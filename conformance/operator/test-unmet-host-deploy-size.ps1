#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of v0.09's changes to `deploy` (docs/operator.md, "deploy" and its steps 4 and 5,
# "Command line", "The host registry, as the operator uses it", "Which checks show it"; docs/state.md, "The
# host registry", "The instance type", "The provisioning run's lock"; D66 to D71; pins P2c, P5, P6, P11,
# P22, P23, P30, P31, P32, P34, P36, P45). Card #0206 (E0.09-T04-S02), against the rows of operator.md's
# "Which checks show it" that name it: the command line with -InstanceType for every host; the box's
# arguments; the record of the type; a failure after a successful `cloudformation deploy`; a person's
# deploy's step 4, with and without a registry; a person's deploy refused while its host's provision.lock
# is held; and a plain deploy of the box not changing its type.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call, the host registry's text as the call found it, and the projects whose provision.lock
# was held when the call arrived, and answer from files beside them (conformance/operator/
# host-commands-common.ps1). Most cases end `deploy` at step 5 with a stack that has no InstanceId output,
# so that `sync` never runs; the cases that run it use ssh and scp stand-ins that succeed, and PATH holds
# no go. A case that needs a lock held at a moment the operator itself reaches (hosts.lock after step 4's
# write, or after step 5's removal) starts a watcher in the case's own runspace, which takes the lock when
# the stand-in aws's log shows the call before that moment, and lets it go when the run has ended.
#
# Which rows other files show: v0.07's cases for `deploy -Host <project>`'s arguments, creating and
# updating, with and without -InstanceType, are test-unmet-host-deploy.ps1's, kept passing; the help copies
# are test-unmet-usage-help.ps1's; check 5's refusals for every other command are
# test-unmet-host-commands-line.ps1's, and this file adds the ones its row names that file does not show.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every ID and key is made up, and a failure reason
# never quotes the program's output. Every write and delete this file makes is checked at the call by
# Assert-ConformancePath, the lock and watcher helpers of host-commands-common.ps1 included.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-deploy-size.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$deployFailed = 'agentspace: Stack deploy failed; see the stack events in the console.'
$templatePattern = 're:\\\.agentspace\\operator\\[0-9a-f]{32}\\agentspace\.yaml$'

# The `cloudformation deploy` call of Calls by command, as a list of arguments (as test-unmet-host-deploy.ps1's).
function Get-DeployCall([string]$Stack, [string[]]$Overrides = @()) {
  $a = @('cloudformation', 'deploy', '--region', 'us-west-2', '--stack-name', $Stack, '--template-file', $templatePattern,
    '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$Stack")
  if ($Overrides.Count -gt 0) { $a += '--parameter-overrides'; $a += $Overrides }
  , $a
}
function Get-DoesNotExist([string]$Stack) { "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $Stack does not exist" }
# A describe-stacks answer written out, so that a case can give the Parameters member any form.
function New-RawStack([string]$Name, [string]$Id, [string]$Params, [string]$Status = 'UPDATE_COMPLETE') {
  $outputs = if ($Id) { '{"OutputKey":"InstanceId","OutputValue":"' + $Id + '"}' } else { '' }
  $o = '{"StackName":"' + $Name + '","StackStatus":"' + $Status + '","Outputs":[' + $outputs + ']'
  if ($Params) { $o += ',"Parameters":' + $Params }
  '{"Stacks":[' + $o + '}]}'
}

Invoke-HostFile -FileName 'test-unmet-host-deploy-size.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox; $idA = $script:IdA; $idC = $script:IdC; $idNew = $script:IdNew
  $typeL = 'm9g.large'; $type1 = 'm9g.xlarge'; $type2 = 'm9g.2xlarge'; $type4 = 'm9g.4xlarge'
  $stack = 'agentspace-host-agentspace'
  $boxKey = 'cloudformation describe-stacks agentbox'
  $hostKey = "cloudformation describe-stacks $stack"
  $attempt = '2026-01-01T00:00:00Z'
  $depBox = "Deploying stack 'agentbox' in us-west-2..."
  $depHost = "Deploying stack '$stack' in us-west-2..."
  $noIdBox = "agentspace: Stack 'agentbox' has no InstanceId output."
  $noIdHost = "agentspace: Stack '$stack' has no InstanceId output."
  $noSystem = "agentspace: SystemRoot is not set, so Windows' programs cannot be found."
  $m2 = 'agentspace: -All applies to status, down and sync alone.'
  $m5 = 'agentspace: -InstanceType applies to deploy alone.'
  $aliasOnly = 'agentspace: -Alias applies to up alone.'
  $typeMsg = { param([string]$A) "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '$A'." }
  $utf8 = [Text.UTF8Encoding]::new($false)

  # ---- Records: the box and two other hosts, every member set, so that "every other member and every other
  # record unchanged" has something to keep ----
  function Get-Rec {
    param([string]$Project, [string]$State = 'ready', [string]$Id = '', [string]$Type = '')
    $a = @{ Project = $Project; State = $State; LastAttempt = $attempt }
    if ($Id) { $a.InstanceId = $Id }
    if ($Type) { $a.InstanceType = $Type; $a.Typed = $true } else { $a.NoInstanceType = $true }
    New-HostRecord @a
  }
  $boxInit = Get-Rec 'swimcoach' 'ready' $idBox $type1
  $alphaInit = Get-Rec 'agentspace' 'ready' $idA $typeL
  $zeta = Get-Rec 'zeta' 'failed' $idC ''
  $initial = @($boxInit, $alphaInit, $zeta)
  $initialCreating = @($boxInit, (Get-Rec 'agentspace' 'creating' '' ''), $zeta)
  function Get-ProjectOf([string]$Target) { if ($Target -eq 'box') { 'swimcoach' } else { 'agentspace' } }
  # The registry after a case that changed the target's record alone, the target in the position it started in.
  function Get-Want([string]$Target, [string]$Id, [string]$Type, [string]$State = 'ready') {
    $r = Get-Rec (Get-ProjectOf $Target) $State $Id $Type
    if ($Target -eq 'box') { @($r, $alphaInit, $zeta) } else { @($boxInit, $r, $zeta) }
  }
  $registryIs = { param($Want, [string]$What) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) (Get-HostsBytes $Want))) { "hosts.json is not $What" } }
  # hosts.json valid by state.md, The host registry: its schema, and the three rules the schema cannot hold.
  function Get-RegistryProblems {
    $b = Get-FileBytesOrNull $fx.HostsFile
    if ($null -eq $b) { return 'hosts.json is missing' }
    $text = $utf8.GetString($b)
    if (-not (Test-ConformanceSchema -Json $text -Schema 'state/hosts.state.schema.json')) { return 'hosts.json does not validate against schemas/state/hosts.state.schema.json' }
    if (-not $text.EndsWith("`n") -or $text.EndsWith("`n`n") -or $text.Contains("`r")) { 'hosts.json does not end in exactly one LF, or holds a carriage return' }
    $doc = ConvertFrom-Json -InputObject $text -AsHashtable
    $recs = @($doc.hosts)
    if (@($recs | Where-Object { $_.stack -ceq 'agentbox' }).Count -ne 1 -or @($recs | Where-Object { $_.stack -ceq 'agentbox' -and $_.project -ceq 'swimcoach' }).Count -ne 1) { 'hosts.json does not hold exactly one record whose stack is agentbox, of the project swimcoach' }
    if (@($recs | ForEach-Object { $_.project } | Select-Object -Unique).Count -ne $recs.Count) { 'hosts.json holds two records of one project' }
    foreach ($r in $recs) {
      if ($r.stack -ne 'agentbox' -and $r.stack -cne ('agentspace-host-' + $r.project)) { 'A record''s stack is not agentspace-host-<its project>' }
      if ($r.ContainsKey('instance_type') -and ([string]$r.instance_type -cnotmatch '^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$' -or ([string]$r.instance_type).Length -gt 64)) { 'A record''s instance_type is not of the member''s form' }
    }
  }
  # The aws calls' "<service> <operation>", in order, exactly.
  function Test-Kinds([string[]]$Want) {
    $got = @(Get-AwsCalls | ForEach-Object { $_.a[0] + ' ' + $_.a[1] })
    if (($got -join '|') -cne ($Want -join '|')) { "The AWS CLI's calls are not '$($Want -join ', ')', in that order ($($got.Count) were made)" }
  }
  $kindsBoxStop = @('cloudformation deploy', 'cloudformation describe-stacks')
  $kindsHostStop = @('cloudformation describe-stacks', 'cloudformation deploy', 'cloudformation describe-stacks')
  # The registry's text as the stand-in aws found it at its last call of an operation (null when it found no file).
  function Get-HostsAt([string]$Operation) {
    $c = @(Get-AwsCalls | Where-Object { $_.a[1] -ceq $Operation })
    if ($c.Count -eq 0) { return '<no call>' }
    $c[$c.Count - 1].hosts
  }
  function Get-Text([string[]]$Records) { $utf8.GetString((Get-HostsBytes $Records)) }
  function Test-PathText([string]$Text, [string]$Path) { $Text -ieq $Path -or $Text -ieq (Assert-ConformancePath -ConformanceHome $fx.H -Path $Path) }
  function Get-ArgsOf([string]$Target, [string]$Type) {
    $a = @('deploy')
    if ($Target -eq 'host') { $a += '-Host', 'agentspace' }
    if ($Type) { $a += '-InstanceType', $Type }
    , $a
  }
  function Get-DepLine([string]$Target) { if ($Target -eq 'box') { $depBox } else { $depHost } }
  function Get-StackName([string]$Target) { if ($Target -eq 'box') { 'agentbox' } else { $stack } }
  # The stand-in's answers for a target: the existence check's, for a host, then the steps given for step 5.
  function Get-Scn([string]$Target, $Steps, $DeployStep = $null) {
    $s = if ($Target -eq 'box') { @{ $boxKey = @($Steps) } } else { @{ $hostKey = (@((New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE'))) + @($Steps)) } }
    if ($null -ne $DeployStep) { $s['cloudformation deploy'] = @($DeployStep) }
    $s
  }
  function Set-Target([string[]]$Records, [hashtable]$Scenario) { Set-HostsRecords $Records; Set-AwsScenario $Scenario }
  # A deploy that goes to its end, sync included, for the box, or for the host of the project agentspace.
  function Get-SuccessScn([string]$Target, [string]$Type, [switch]$Create) {
    $doc = New-Step (New-StackDoc -StackName (Get-StackName $Target) -Id $idNew -Status $(if ($Create) { 'CREATE_COMPLETE' } else { 'UPDATE_COMPLETE' }) -InstanceType $Type)
    $steps = if ($Target -eq 'box') { @($doc) } elseif ($Create) { @((New-Step '' (Get-DoesNotExist $stack) 254), $doc) } else { @($doc) }
    Merge-Scenario (Get-HealthyAnswers -Id $idNew) @{ $(if ($Target -eq 'box') { $boxKey } else { $hostKey }) = $steps }
  }
  function Test-Usage([string]$Name, [AllowEmptyString()][string[]]$Arguments, [string]$Message, [scriptblock]$Setup = {}, [hashtable]$Environment = @{}) {
    Test-HostCase $Name $Arguments @{ Exit = 1; Stderr = "$Message`n"; Calls = @() } -Setup $Setup -Environment $Environment
  }
  $setupInitial = { Set-HostsRecords $initial }
  $paramsWithType = { Set-ParamsFile ('{"version":0,"parameters":{"VpcId":"' + $fx.VpcValue + '","SubnetId":"' + $fx.SubnetValue + '","InstanceType":"' + $type4 + '","RootVolumeGiB":"40"}}') }
  $paramsWithout = { Set-ParamsFile ('{"version":0,"parameters":{"SubnetId":"' + $fx.SubnetValue + '","VpcId":"' + $fx.VpcValue + '","RootVolumeGiB":"40"}}') }

  # ======================================================================================================
  # Row: A plain deploy of the box does not change its type when stack-parameters.json names another
  # ======================================================================================================
  $r9 = 'deploy-plain-box'
  Test-HostCase "${r9}: a plain deploy of the box, with stack-parameters.json naming InstanceType $type4 and the box's stack at $type1, passes no InstanceType and none of the file's value, leaves the file, and records $type1, the stack's" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $d = @(Get-AwsCalls | Where-Object { $_.a[1] -ceq 'deploy' })
      if ($d.Count -ne 1) { return 'cloudformation deploy was not called once' }
      if (@($d[0].a | Where-Object { $_ -cmatch 'InstanceType' -or $_ -cmatch [regex]::Escape($type4) }).Count -gt 0) { 'The box''s plain deploy passed an InstanceType, or the file''s value' }
      if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.ParamsFile) $script:ParamsBytes)) { 'stack-parameters.json was changed' }
      & $registryIs (Get-Want 'box' $idNew $type1) 'the box''s record holding the stack''s type, not the file''s'
      Get-RegistryProblems
      } } -Setup {
    Set-Target $initial (Get-SuccessScn 'box' $type1)
    & $paramsWithType
    $script:ParamsBytes = Get-FileBytesOrNull $fx.ParamsFile
  }

  # ======================================================================================================
  # Row: The command line (operator.md, "Which checks show it", the first v0.09 row)
  # ======================================================================================================
  $r1 = 'deploy-line'
  # Each line passes the host checks and goes on to the AWS stand-in: the deploy of the box's stack with
  # InstanceType last, and the read of its stack. Nothing on stderr names -InstanceType (v0.07's check 6
  # printed `-InstanceType needs -Host <project> of a host that is not the box; ...`, which no line prints).
  $v07Check6 = @('needs -Host <project>', 'is sized through stack-parameters.json', 'not the box')
  foreach ($case in @(
      @('with no -Host', @('deploy', '-InstanceType', $type2)),
      @('with -Host swimcoach', @('deploy', '-Host', 'swimcoach', '-InstanceType', $type2)),
      @('with -Host SwimCoach, in capitals', @('deploy', '-Host', 'SwimCoach', '-InstanceType', $type2)),
      @('with -Host padded with whitespace', @('deploy', '-Host', ' SwimCoach ', '-InstanceType', $type2)),
      @('with -InstanceType before -Host swimcoach', @('deploy', '-InstanceType', $type2, '-Host', 'swimcoach')),
      @('with -Quiet before it', @('deploy', '-Quiet', '-InstanceType', $type2))
    )) {
    $quiet = $case[1] -contains '-Quiet'
    $errText = if ($quiet) { "$noIdBox`n" } else { Join-Lines @($depBox, $noIdBox) }
    Test-HostCase "${r1}: deploy -InstanceType $type2 $($case[0]) passes the host checks and reaches the AWS stand-in, with the box's stack" ([string[]]$case[1]) @{
      Exit = 1; Stderr = $errText; Calls = 'any'
      Check = { param($run, $started)
        $c = @(Get-AwsCalls)
        if ($c.Count -ne 2 -or $c[0].a[1] -cne 'deploy' -or $c[1].Text -cne (Get-CallStack 'agentbox')) { 'The AWS stand-in was not reached by the deploy of agentbox and the read of its stack' }
        elseif ($c[0].a[$c[0].a.Count - 1] -cne "InstanceType=$type2") { 'The last argument of cloudformation deploy is not InstanceType=<the type>' }
        foreach ($bad in $v07Check6) { if ($run.Stderr.Contains($bad)) { "Stderr holds v0.07's check-6 text '$bad'" } } } } -Setup $setupInitial
  }
  Test-HostCase "${r1}: deploy -InstanceType $type2 with -Host <project> passes the host checks and reaches the AWS stand-in, with the project's stack" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 1; Stderr = (Join-Lines @($depHost, $noIdHost)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack @("InstanceType=$type2")), (Get-CallStack $stack)) } -Setup {
    Set-Target $initial @{ $hostKey = @(New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')) }
  }
  Test-HostCase "${r1}: deploy -InstanceType $type2 with -Host Agentspace, in capitals, finds the project's record and reaches the AWS stand-in" @('deploy', '-Host', 'Agentspace', '-InstanceType', $type2) @{
    Exit = 1; Stderr = (Join-Lines @($depHost, $noIdHost)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack @("InstanceType=$type2")), (Get-CallStack $stack)) } -Setup {
    Set-Target $initial @{ $hostKey = @(New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')) }
  }
  # An instance type the EC2 driver's map holds no size for is accepted: the operator knows no size (D67).
  Test-HostCase "${r1}: deploy -InstanceType m7i.metal-24xl, a type of the form that no size maps to, is passed on as it is" @('deploy', '-InstanceType', 'm7i.metal-24xl') @{
    Exit = 1; Calls = 'any'
    Check = { param($run, $started)
      $c = @(Get-AwsCalls)
      if ($c.Count -ne 2 -or $c[0].a[$c[0].a.Count - 1] -cne 'InstanceType=m7i.metal-24xl') { 'The type was not passed on as the last override of cloudformation deploy' } } } -Setup $setupInitial
  # Going on to the home and the system folder: with SystemRoot unset, each line gives that message, and
  # nothing is written, a hosts folder and a provision.lock included (the lock is taken after them).
  foreach ($case in @(
      @('with no -Host', @('deploy', '-InstanceType', $type2)),
      @('with -Host swimcoach', @('deploy', '-Host', 'swimcoach', '-InstanceType', $type2)),
      @('with -Host SwimCoach', @('deploy', '-Host', 'SwimCoach', '-InstanceType', $type2)),
      @('with -Host <project>', @('deploy', '-Host', 'agentspace', '-InstanceType', $type2))
    )) {
    Test-Usage "${r1}: deploy -InstanceType $type2 $($case[0]) goes on to the home and system folder checks: with SystemRoot unset it reports the system folder, makes no call and writes no lock" ([string[]]$case[1]) $noSystem -Setup $setupInitial -Environment @{ SystemRoot = $null }
  }
  Test-HostCase "${r1}: the system folder check writes nothing for deploy -InstanceType: no hosts folder, no alias, no ssh config, hosts.json as it was" @('deploy', '-InstanceType', $type2) @{
    Exit = 1; Stderr = "$noSystem`n"; Calls = @()
    Check = { param($run, $started)
      Get-NothingSignedProblems
      & $registryIs $initial 'as it was' } } -Setup $setupInitial -Environment @{ SystemRoot = $null }
  # Check 5 unchanged: every other command given -InstanceType, with the box's forms and a project with no record.
  foreach ($case in @(
      @('up -Host swimcoach', @('up', '-Host', 'swimcoach', '-InstanceType', $type2)),
      @('status -Host swimcoach', @('status', '-Host', 'swimcoach', '-InstanceType', $type2)),
      @('grow -Host swimcoach', @('grow', '-Host', 'swimcoach', '-InstanceType', $type2)),
      @('down', @('down', '-InstanceType', $type2)),
      @('sync -Host agentspace', @('sync', '-Host', 'agentspace', '-InstanceType', $type2)),
      @('bootstrap', @('bootstrap', '-InstanceType', $type2)),
      @('guard', @('guard', '-InstanceType', $type2)),
      @('ca', @('ca', '-InstanceType', $type2)),
      @('provision -Host agentspace', @('provision', '-Host', 'agentspace', '-InstanceType', $type2)),
      @('provision -Host nobody, a project with no record', @('provision', '-Host', 'nobody', '-InstanceType', $type2)),
      @('provision -Host swimcoach', @('provision', '-Host', 'swimcoach', '-InstanceType', $type2))
    )) {
    Test-Usage "${r1}: check 5 is unchanged: $($case[0]) given -InstanceType is refused with 'applies to deploy alone'" ([string[]]$case[1]) $m5 -Setup $setupInitial
  }
  Test-Usage "${r1}: check 5 comes before the home and system folder checks: status -InstanceType with SystemRoot unset" @('status', '-InstanceType', $type2) $m5 -Setup $setupInitial -Environment @{ SystemRoot = $null }
  Test-Usage "${r1}: check 5 comes before the host's lookup: up -Host nobody -InstanceType" @('up', '-Host', 'nobody', '-InstanceType', $type2) $m5 -Setup $setupInitial
  Test-Usage "${r1}: check 5 comes after the -Alias checks: status -Alias x -InstanceType" @('status', '-Alias', 'x', '-InstanceType', $type2) $aliasOnly -Setup $setupInitial
  Test-Usage "${r1}: check 5 comes after check 2, which deploy is not exempt from: deploy -All -InstanceType" @('deploy', '-All', '-InstanceType', $type2) $m2 -Setup $setupInitial
  Test-Usage "${r1}: check 5 comes after check 2 for provision: provision -All -InstanceType" @('provision', '-All', '-InstanceType', $type2) $m2 -Setup $setupInitial
  # -InstanceType's form check unchanged, for deploy given each form of host.
  foreach ($case in @(
      @('with no -Host', @()),
      @('with -Host swimcoach', @('-Host', 'swimcoach')),
      @('with -Host <project>', @('-Host', 'agentspace'))
    )) {
    foreach ($bad in 'Bad', 'M9g.xlarge', 'm9g.', 'm9g', 'm9g.x_large', ' m9g.2xlarge') {
      Test-Usage "${r1}: the form check is unchanged: deploy $($case[0]) -InstanceType '$bad' is refused with the form message" ([string[]](@('deploy') + $case[1] + @('-InstanceType', $bad))) (& $typeMsg $bad) -Setup $setupInitial
    }
  }
  Test-Usage "${r1}: the form check comes before the system folder check: deploy -InstanceType Bad with SystemRoot unset" @('deploy', '-InstanceType', 'Bad') (& $typeMsg 'Bad') -Setup $setupInitial -Environment @{ SystemRoot = $null }
  Test-Usage "${r1}: the form check comes before the host's lookup: deploy -Host nobody -InstanceType Bad" @('deploy', '-Host', 'nobody', '-InstanceType', 'Bad') (& $typeMsg 'Bad') -Setup $setupInitial
  Test-Usage "${r1}: the form check comes before the host checks: deploy -All -InstanceType Bad" @('deploy', '-All', '-InstanceType', 'Bad') (& $typeMsg 'Bad') -Setup $setupInitial
  Test-Usage "${r1}: -InstanceType with no value, as the last argument of the box's deploy" @('deploy', '-InstanceType') 'agentspace: -InstanceType needs a value.' -Setup $setupInitial
  Test-Usage "${r1}: -InstanceType given twice to the box's deploy" @('deploy', '-InstanceType', $type2, '-InstanceType', $type4) 'agentspace: -InstanceType is given more than once.' -Setup $setupInitial
  Test-HostCase "${r1}: deploy -Host nobody -InstanceType <type> is refused for the project's missing record, after the form check and with no AWS call" @('deploy', '-Host', 'nobody', '-InstanceType', $type2) @{
    Exit = 1; Stderr = "agentspace: Project 'nobody' has no host; provision -Host nobody makes one.`n"; Calls = @() } -Setup $setupInitial

  # ======================================================================================================
  # Row: The box's deploy arguments
  # ======================================================================================================
  $r2 = 'deploy-args box'
  # SshPublicKey first, then every member of the file but InstanceType (no order is fixed for them), then
  # InstanceType=<type> last when -InstanceType is given; no HostName, no CaPublicKey; the file untouched.
  function Get-BoxArgsProblems([string]$Want, [bool]$FileNames) {
    $c = @(Get-AwsCalls)
    if ($c.Count -ne 2) { return "The AWS CLI was called $($c.Count) times, not 2: the deploy and the read of its stack, with no existence check" }
    if ($c[0].a[1] -cne 'deploy' -or $c[1].Text -cne (Get-CallStack 'agentbox')) { return 'The two calls are not the deploy of agentbox and the read of its stack' }
    $a = $c[0].a
    $i = [array]::IndexOf($a, '--parameter-overrides')
    if ($i -lt 1) { return 'The box''s deploy passed no --parameter-overrides' }
    if (-not (Test-CallMatch (Get-DeployCall 'agentbox' @()) @($a[0..($i - 1)]))) { 'The arguments before --parameter-overrides are not the box''s (stack, template, capability, flag, tag project=agentbox)' }
    $ov = @($a[($i + 1)..($a.Count - 1)])
    if ($ov[0] -cne "SshPublicKey=$($fx.PubKey)") { 'The first override is not SshPublicKey=<the public key>' }
    $wantCount = 4 + $(if ($Want) { 1 } else { 0 })
    if ($ov.Count -ne $wantCount) { "There are $($ov.Count) overrides, not the $wantCount of SshPublicKey, every other member of the file but InstanceType$(if ($Want) { ', and InstanceType last' })" }
    $middle = @($ov | Select-Object -Skip 1 | Where-Object { $_ -cnotmatch '^InstanceType=' } | Sort-Object)
    $expect = @("RootVolumeGiB=40", "SubnetId=$($fx.SubnetValue)", "VpcId=$($fx.VpcValue)" | Sort-Object)
    if (($middle -join '|') -cne ($expect -join '|')) { 'The overrides between SshPublicKey and the end are not every other member of the file, and nothing else (no HostName, no CaPublicKey)' }
    $typed = @($ov | Where-Object { $_ -cmatch '^InstanceType=' })
    if ($Want) {
      if ($ov[-1] -cne "InstanceType=$Want") { 'The last override is not InstanceType=<the type -InstanceType gave>' }
      if ($typed.Count -ne 1) { 'InstanceType is passed more than once' }
    } elseif ($typed.Count -ne 0) { 'InstanceType is passed with no -InstanceType, from the file or from anywhere' }
    if (($a -join ' ').Contains($type4)) { 'The file''s InstanceType value appears in the box''s deploy arguments' }
    if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.ParamsFile) $script:ParamsBytes)) { 'stack-parameters.json was changed' }
  }
  foreach ($line in @(@('deploy'), @('deploy', '-Host', 'swimcoach'), @('deploy', '-Host', 'SwimCoach'))) {
    foreach ($fileNames in $true, $false) {
      foreach ($want in '', $type2) {
        $fn = $fileNames; $wt = $want
        $lineArgs = @($line) + $(if ($wt) { @('-InstanceType', $wt) } else { @() })
        $fileText = if ($fn) { 'a file that names InstanceType' } else { 'a file that does not name it' }
        $typeText = if ($wt) { "with -InstanceType ${wt}: InstanceType=${wt} last" } else { 'with no -InstanceType: InstanceType not passed' }
        Test-HostCase "${r2}: $($lineArgs -join ' ') with $fileText, $typeText, and the file left byte for byte" ([string[]]$lineArgs) @{
          Exit = 1; Stderr = (Join-Lines @($depBox, $noIdBox)); Calls = 'any'
          Check = { param($run, $started) Get-BoxArgsProblems $wt $fn } } -Setup {
          Set-Target $initial @{ $boxKey = @(New-Step (New-StackDoc -StackName 'agentbox')) }
          if ($fn) { & $paramsWithType } else { & $paramsWithout }
          $script:ParamsBytes = Get-FileBytesOrNull $fx.ParamsFile
        }
      }
    }
  }
  # The same, with the file's InstanceType and -InstanceType both present: the flag's value alone is passed.
  Test-HostCase "${r2}: a file that names InstanceType $type4 and -InstanceType $type2 together pass InstanceType=$type2 alone" @('deploy', '-InstanceType', $type2) @{
    Exit = 1; Calls = 'any'; Check = { param($run, $started) Get-BoxArgsProblems $type2 $true } } -Setup {
    Set-Target $initial @{ $boxKey = @(New-Step (New-StackDoc -StackName 'agentbox')) }
    & $paramsWithType
    $script:ParamsBytes = Get-FileBytesOrNull $fx.ParamsFile
  }

  # ======================================================================================================
  # Row: deploy -Host <project>'s arguments (v0.07's cases, kept passing, are test-unmet-host-deploy.ps1's).
  # What they do not show: a file that names InstanceType beside a host's stack.
  # ======================================================================================================
  $r3 = 'deploy-args host'
  Test-HostCase "${r3}: a host's stack it updates, with a file that names InstanceType and no -InstanceType, passes no parameter at all, and leaves the file" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Stderr = (Join-Lines @($depHost, $noIdHost)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack), (Get-CallStack $stack))
    Check = { param($run, $started) if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.ParamsFile) $script:ParamsBytes)) { 'stack-parameters.json was changed' } } } -Setup {
    Set-Target $initial @{ $hostKey = @(New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')) }
    & $paramsWithType
    $script:ParamsBytes = Get-FileBytesOrNull $fx.ParamsFile
  }
  Test-HostCase "${r3}: a host's stack it updates, with a file that names InstanceType and -InstanceType $type2, passes InstanceType=$type2 alone" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 1; Stderr = (Join-Lines @($depHost, $noIdHost)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack @("InstanceType=$type2")), (Get-CallStack $stack)) } -Setup {
    Set-Target $initial @{ $hostKey = @(New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')) }
    & $paramsWithType
  }
  $ca = ([IO.File]::ReadAllText((Join-Path $fx.CaDir 'agentspace_ca.pub')).Trim() -split '\s+')[0..1] -join ' '
  $createOverrides = @("SshPublicKey=$($fx.PubKey)", "SubnetId=$($fx.SubnetValue)", "VpcId=$($fx.VpcValue)", "HostName=$stack", "CaPublicKey=$ca")
  Test-HostCase "${r3}: a new host's stack, with a file that names InstanceType $type4 and -InstanceType $type2, passes the creation parameters and InstanceType=$type2 last, and not the file's value" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 1; Calls = @((Get-CallStack $stack), (Get-DeployCall $stack ($createOverrides + "InstanceType=$type2")), (Get-CallStack $stack)) } -Setup {
    Set-Target $initialCreating @{ $hostKey = @((New-Step '' (Get-DoesNotExist $stack) 254), (New-Step (New-StackDoc -StackName $stack))) }
    & $paramsWithType
  }

  # ======================================================================================================
  # Row: The record of the type, after a successful cloudformation deploy
  # ======================================================================================================
  $r4 = 'deploy-record'
  # The state the removal at step 5 leaves, seen by the stand-in aws at its describe-stacks call: the target
  # with neither instance_id nor instance_type, every other member and record as they were.
  function Get-RemovalProblems([string]$Target) {
    $want = Get-Text (Get-Want $Target '' '')
    if ((Get-HostsAt 'describe-stacks') -cne $want) { 'The record the stack was read under is not the target''s with neither instance_id nor instance_type, every other member and record as they were' }
  }
  Test-HostCase "${r4}: the box's plain deploy writes the stack's InstanceType $type2, read back, beside the new instance_id, over a record that held $type1, and every other member and record is as it was" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type2) 'the box''s record holding the new instance ID and the stack''s type, every other record as it was'
      Get-RegistryProblems
      Get-RemovalProblems 'box'
      $c = @(Get-AwsCalls)
      if ($c.Count -ge 1 -and $c[0].hosts -cne (Get-Text $initial)) { 'A plain deploy wrote the registry before cloudformation deploy' } } } -Setup {
    Set-Target $initial (Get-SuccessScn 'box' $type2)
  }
  Test-HostCase "${r4}: the box's deploy -InstanceType $type2 records the type the stack reports, $type2" @('deploy', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type2) "the box's record holding the new instance ID and $type2, every other record as it was"
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'box' $type2) }
  Test-HostCase "${r4}: the type is the stack's, never -InstanceType's: deploy -InstanceType $type2 with a stack that reports $type4 records $type4" @('deploy', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type4) 'the box''s record holding the type the stack reported'
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'box' $type4) }
  Test-HostCase "${r4}: deploy -Host swimcoach -InstanceType $type2 records the type for the box, as the box's own deploy does" @('deploy', '-Host', 'swimcoach', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type2) 'the box''s record holding the new instance ID and the stack''s type'
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'box' $type2) }
  Test-HostCase "${r4}: the box's record that held no instance_type gains the stack's, with the instance_id, from a plain deploy" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type1) 'the box''s record holding the new instance ID and the stack''s type'
      Get-RegistryProblems } } -Setup {
    Set-Target @((Get-Rec 'swimcoach' 'ready' $idBox ''), $alphaInit, $zeta) (Get-SuccessScn 'box' $type1)
  }
  Test-HostCase "${r4}: another host's plain deploy of a stack it updates records the stack's type $type4 and the new instance_id in that host's record alone" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'host' $idNew $type4) 'the host''s record holding the new instance ID and the stack''s type, the box''s and every other record as they were'
      Get-RegistryProblems
      Get-RemovalProblems 'host' } } -Setup { Set-Target $initial (Get-SuccessScn 'host' $type4) }
  Test-HostCase "${r4}: another host's deploy -InstanceType $type2 of a stack it updates records the stack's type $type2" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'host' $idNew $type2) "the host's record holding the new instance ID and $type2"
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'host' $type2) }
  Test-HostCase "${r4}: another host's deploy -InstanceType $type2 of a stack it creates records the stack's type in a record that stays creating, with its last_attempt_at and no other change" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'host' $idNew $type2 'creating') 'the host''s record, still creating, holding the new instance ID and the stack''s type'
      Get-RegistryProblems } } -Setup { Set-Target $initialCreating (Get-SuccessScn 'host' $type2 -Create) }
  Test-HostCase "${r4}: a deploy of a new stack with no -InstanceType records the type the stack reports, the template's default" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'host' $idNew $type1 'creating') 'the host''s record holding the new instance ID and the stack''s type'
      Get-RegistryProblems } } -Setup { Set-Target $initialCreating (Get-SuccessScn 'host' $type1 -Create) }
  $long64 = 'm9g.' + ('x' * 60)
  Test-HostCase "${r4}: a type of 64 characters, the member's longest, is recorded as the stack reports it" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $long64) 'the box''s record holding the 64-character type'
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'box' $long64) }
  Test-HostCase "${r4}: a type the stand-in changes between two deploys is recorded anew by the second" @('deploy', '-InstanceType', $type2) @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'box' $idNew $type2) "the box's record holding $type2 after the first deploy"
      Get-RegistryProblems
      Clear-StandinLogs
      Set-AwsScenario (Get-SuccessScn 'box' $type4)
      $r2 = Invoke-HostRun -Arguments @('deploy')
      if ($r2.ExitCode -ne 0) { 'The second deploy did not succeed' }
      & $registryIs (Get-Want 'box' $idNew $type4) "the box's record holding $type4 after the second deploy"
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'box' $type2) }
  Test-HostCase "${r4}: the second deploy of a host, after a first that recorded $type2, records the type the stack now reports, $typeL" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      & $registryIs (Get-Want 'host' $idNew $type2) "the host's record holding $type2 after the first deploy"
      Clear-StandinLogs
      Set-AwsScenario (Get-SuccessScn 'host' $typeL)
      $r2 = Invoke-HostRun -Arguments @('deploy', '-Host', 'agentspace')
      if ($r2.ExitCode -ne 0) { 'The second deploy did not succeed' }
      & $registryIs (Get-Want 'host' $idNew $typeL) "the host's record holding $typeL after the second deploy"
      Get-RegistryProblems } } -Setup { Set-Target $initial (Get-SuccessScn 'host' $type2) }
  # In one write: the watcher polls hosts.json while the deploy runs and sees every text it holds. Neither
  # an instance_id without an instance_type nor the reverse is ever there for the target. (Polling can miss
  # a state that lasts a millisecond; the three states that last seconds, before, between and after, MUST
  # be seen, so the watcher is shown to see.)
  foreach ($target in 'box', 'host') {
    $tg = $target
    Test-HostCase "${r4}: the instance_id and the instance_type are written in one write ($tg): no text of hosts.json holds one without the other, and the watcher saw the record before, with neither, and after" (Get-ArgsOf $tg $type2) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $seen = @(Stop-HostsWatcher)
        $proj = Get-ProjectOf $tg
        $states = @($seen | ForEach-Object { $id = Get-RecordMember $_ $proj 'instance_id'; $ty = Get-RecordMember $_ $proj 'instance_type'; "$([bool]$id)/$([bool]$ty)" })
        # A record never holds a type without an instance_id. Step 4 leaves an instance_id without a type (with
        # -InstanceType); after the removal of step 5 leaves neither, the next text holds both, never one alone.
        if ($states -ccontains 'False/True') { "hosts.json held the record of '$proj' with an instance_type and no instance_id" }
        $k = [array]::IndexOf($states, 'False/False')
        if ($k -lt 0) { 'The watcher never saw the record with neither instance_id nor instance_type' }
        elseif ($k -ge $states.Count - 1) { 'The watcher never saw the record after the removal of step 5' }
        else { foreach ($after in @($states[($k + 1)..($states.Count - 1)])) { if ($after -cne 'True/True') { "After the removal of step 5, hosts.json held the record of '$proj' with only one of instance_id and instance_type" } } }
        if ($states.Count -lt 3) { 'The watcher saw fewer than three states of hosts.json: before, with neither, and after' }
        & $registryIs (Get-Want $tg $idNew $type2) 'the target''s record holding the new instance ID and the stack''s type' } } -Setup {
      Set-Target $initial (Get-SuccessScn $tg $type2)
      Start-HostsWatcher
    }
  }

  # ======================================================================================================
  # Row: A failure after a successful cloudformation deploy
  # ======================================================================================================
  $r5 = 'deploy-fail-after'
  foreach ($target in 'box', 'host') {
    foreach ($withType in $false, $true) {
      $tg = $target; $ty = $(if ($withType) { $type2 } else { '' })
      $dep = Get-DepLine $tg; $sn = Get-StackName $tg
      $kinds = if ($tg -eq 'box') { $kindsBoxStop } else { $kindsHostStop }
      $lab = "$tg, $(if ($ty) { "with -InstanceType $ty" } else { 'with no -InstanceType' })"
      $argv = Get-ArgsOf $tg $ty
      # describe-stacks fails: its text, exit 1, no instance_id and no instance_type (a plain deploy loses its type here too, pin P30), no sync.
      Test-HostCase "${r5}: a describe-stacks that fails after a successful cloudformation deploy fails deploy with the CLI's text, exit 1, and leaves the record with no instance_id and no instance_type, and runs no sync ($lab)" $argv @{
        Exit = 1; Stderr = (Join-Lines @($dep, 'agentspace: An error occurred (AccessDenied) when calling the DescribeStacks operation: User is not authorized')); Calls = 'any'
        Check = { param($run, $started) Test-Kinds $kinds; & $registryIs (Get-Want $tg '' '') 'the target''s record with no instance_id and no instance_type, every other record as it was'; Get-RegistryProblems } } -Setup {
        Set-Target $initial (Get-Scn $tg @((New-Step '' 'An error occurred (AccessDenied) when calling the DescribeStacks operation: User is not authorized' 254)))
      }
      Test-HostCase "${r5}: a describe-stacks that fails because the sign-in has expired prints exactly the sign-in message there, with the same record ($lab)" $argv @{
        Exit = 1; Stderr = (Join-Lines @($dep, $signIn)); Calls = 'any'
        Check = { param($run, $started) Test-Kinds $kinds; & $registryIs (Get-Want $tg '' '') 'the target''s record with no instance_id and no instance_type' } } -Setup {
        Set-Target $initial (Get-Scn $tg @((New-Step '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255)))
      }
      Test-HostCase "${r5}: an answer with no InstanceId output writes neither the instance_id nor the instance_type, whatever it says of the type ($lab)" $argv @{
        Exit = 1; Stderr = (Join-Lines @($dep, "agentspace: Stack '$sn' has no InstanceId output.")); Calls = 'any'
        Check = { param($run, $started) Test-Kinds $kinds; & $registryIs (Get-Want $tg '' '') 'the target''s record with neither'; Get-RegistryProblems } } -Setup {
        Set-Target $initial (Get-Scn $tg @((New-Step (New-StackDoc -StackName $sn -InstanceType $type2))))
      }
      Test-HostCase "${r5}: the InstanceId output is checked first: no InstanceId and no InstanceType parameter gives the InstanceId message and writes neither ($lab)" $argv @{
        Exit = 1; Stderr = (Join-Lines @($dep, "agentspace: Stack '$sn' has no InstanceId output.")); Calls = 'any'
        Check = { param($run, $started) & $registryIs (Get-Want $tg '' '') 'the target''s record with neither' } } -Setup {
        Set-Target $initial (Get-Scn $tg @((New-Step (New-RawStack $sn '' '[]'))))
      }
    }
  }
  # An InstanceType parameter that is missing, malformed or doubled: the message of P22, the instance_id written alone,
  # no instance_type, no `Instance: <id>` line and no sync.
  $variants = @(
    @('no InstanceType element', '[]'),
    @('no Parameters member at all', ''),
    @('only another parameter', '[{"ParameterKey":"SubnetId","ParameterValue":"x"}]'),
    @('an element with no ParameterValue', '[{"ParameterKey":"InstanceType"}]'),
    @('a ParameterValue that is a number', '[{"ParameterKey":"InstanceType","ParameterValue":5}]'),
    @('a ParameterValue that is null', '[{"ParameterKey":"InstanceType","ParameterValue":null}]'),
    @('a ParameterValue that is a list', '[{"ParameterKey":"InstanceType","ParameterValue":["m9g.xlarge"]}]'),
    @('a ParameterValue in capitals', '[{"ParameterKey":"InstanceType","ParameterValue":"M9G.xlarge"}]'),
    @('a ParameterValue with no dot', '[{"ParameterKey":"InstanceType","ParameterValue":"m9g"}]'),
    @('a ParameterValue that ends in a line feed', '[{"ParameterKey":"InstanceType","ParameterValue":"m9g.xlarge\n"}]'),
    @('a ParameterValue of 65 characters', ('[{"ParameterKey":"InstanceType","ParameterValue":"m9g.' + ('x' * 61) + '"}]')),
    @('an empty ParameterValue', '[{"ParameterKey":"InstanceType","ParameterValue":""}]'),
    @('a ParameterValue with a space', '[{"ParameterKey":"InstanceType","ParameterValue":"m9g.2x large"}]'),
    @('two InstanceType elements, the same value', '[{"ParameterKey":"InstanceType","ParameterValue":"m9g.xlarge"},{"ParameterKey":"InstanceType","ParameterValue":"m9g.xlarge"}]'),
    @('two InstanceType elements, two values', '[{"ParameterKey":"InstanceType","ParameterValue":"m9g.xlarge"},{"ParameterKey":"InstanceType","ParameterValue":"m9g.2xlarge"}]')
  )
  # Every variant for the box with no -InstanceType; the first of each kind again with -InstanceType, and for another host.
  $again = @('no InstanceType element', 'a ParameterValue that is a number', 'a ParameterValue in capitals', 'two InstanceType elements, the same value')
  foreach ($v in $variants) {
    $plan = @(, @('box', ''))
    if ($again -contains $v[0]) { $plan += , @('box', $type2); $plan += , @('host', '') }
    foreach ($p in $plan) {
      $tg = $p[0]; $ty = $p[1]; $params = $v[1]
      $sn = Get-StackName $tg; $dep = Get-DepLine $tg
      $kinds = if ($tg -eq 'box') { $kindsBoxStop } else { $kindsHostStop }
      $lab = "$tg, $(if ($ty) { "with -InstanceType $ty" } else { 'with no -InstanceType' })"
      Test-HostCase "${r5}: $($v[0]) fails deploy with the no-InstanceType-parameter message, writes the instance_id alone and no instance_type, prints no Instance line and runs no sync ($lab)" (Get-ArgsOf $tg $ty) @{
        Exit = 1; Stderr = (Join-Lines @($dep, "agentspace: Stack '$sn' has no InstanceType parameter.")); Calls = 'any'
        Check = { param($run, $started)
          Test-Kinds $kinds
          & $registryIs (Get-Want $tg $idNew '') 'the target''s record holding the new instance_id and no instance_type, every other record as it was'
          Get-RegistryProblems } } -Setup {
        Set-Target $initial (Get-Scn $tg @((New-Step (New-RawStack $sn $idNew $params))))
      }
    }
  }
  # hosts.lock held by the case past its 30 seconds. The lock is taken by the case's watcher once the stand-in
  # aws logs the call before the moment, so that the write before it goes through (docs/operator.md, deploy, step 5).
  function Get-LockProblems($run, [bool]$Acquired) {
    $m = [regex]::Match($run.Stderr.TrimEnd("`n"), '(?m)^agentspace: The host registry''s lock (.+) is held by another program; try again\.$')
    if (-not $m.Success) { 'Stderr has no line agentspace: The host registry''s lock <path> is held by another program; try again.' }
    elseif (-not (Test-PathText $m.Groups[1].Value $fx.HostsLock)) { 'The path in the message is not hosts.lock''s' }
    if (@($run.Stderr -split "`n" | Where-Object { $_ -ne '' })[-1] -cne $m.Value) { 'The lock''s message is not the last line' }
    if ($run.Elapsed -lt 28 -or $run.Elapsed -gt 70) { "The command waited $([int]$run.Elapsed) seconds, not about 30" }
    if (-not $Acquired) { 'The case''s watcher never took hosts.lock, so the lock was not held at the step' }
  }
  foreach ($case in @(
      @('box', '', 'at the removal of step 5'),
      @('box', $type2, 'at the removal of step 5, after step 4''s removal went through'),
      @('host', '', 'at the removal of step 5'),
      @('host', $type2, 'at the removal of step 5, after step 4''s removal went through')
    )) {
    $tg = $case[0]; $ty = $case[1]
    $calls = if ($tg -eq 'box') { @('cloudformation deploy') } else { @('cloudformation describe-stacks', 'cloudformation deploy') }
    $afterCalls = $calls.Count
    # Plain: the record keeps its instance_id and its type. With -InstanceType: step 4 removed the type, the instance_id stays.
    $wantRec = if ($ty) { Get-Want $tg (@($idBox, $idA)[[int]($tg -eq 'host')]) '' } else { $initial }
    Test-HostCase "${r5}: hosts.lock held past its 30 seconds $($case[2]) ($tg, $(if ($ty) { "with -InstanceType $ty" } else { 'with no -InstanceType' })) gives the lock's message, exit 1, no describe-stacks after it, and the record keeps its instance_id$(if (-not $ty) { ' and its instance_type' } else { ', its type removed by step 4' })" (Get-ArgsOf $tg $ty) @{
      Exit = 1; Calls = 'any'
      Check = { param($run, $started)
        $h = Stop-LockHolder
        Get-LockProblems $run ($null -ne $h -and $h.Acquired)
        Test-Kinds $calls
        & $registryIs $wantRec 'the record keeping its instance_id, and its type only without -InstanceType' } } -Setup {
      Set-Target $initial (Get-Scn $tg @((New-Step (New-StackDoc -StackName (Get-StackName $tg) -Id $idNew -InstanceType $type2))))
      Start-LockHolder -AfterCalls $afterCalls
    } -Timeout 150
  }
  foreach ($case in @(
      @('box', '', 'at step 5''s write of the ID and type'),
      @('box', $type2, 'at step 5''s write of the ID and type'),
      @('host', '', 'at step 5''s write of the ID and type'),
      @('host', $type2, 'at step 5''s write of the ID and type')
    )) {
    $tg = $case[0]; $ty = $case[1]
    $calls = if ($tg -eq 'box') { @('cloudformation deploy', 'cloudformation describe-stacks') } else { @('cloudformation describe-stacks', 'cloudformation deploy', 'cloudformation describe-stacks') }
    $afterCalls = $calls.Count
    Test-HostCase "${r5}: hosts.lock held past its 30 seconds $($case[2]), after a removal the case lets through ($tg, $(if ($ty) { "with -InstanceType $ty" } else { 'with no -InstanceType' })), gives the lock's message, exit 1, and the record with neither" (Get-ArgsOf $tg $ty) @{
      Exit = 1; Calls = 'any'
      Check = { param($run, $started)
        $h = Stop-LockHolder
        Get-LockProblems $run ($null -ne $h -and $h.Acquired)
        Test-Kinds $calls
        if ((Get-HostsAt 'describe-stacks') -cne (Get-Text (Get-Want $tg '' ''))) { 'The removal did not go through before the lock was held: the record the stack was read under is not the target''s with neither' }
        & $registryIs (Get-Want $tg '' '') 'the target''s record with neither the instance_id nor the instance_type' } } -Setup {
      Set-Target $initial (Get-Scn $tg @((New-Step (New-StackDoc -StackName (Get-StackName $tg) -Id $idNew -InstanceType $type2))))
      Start-LockHolder -AfterCalls $afterCalls
    } -Timeout 150
  }

  # ======================================================================================================
  # Row: A person's deploy's step 4
  # ======================================================================================================
  $r6 = 'deploy-step4'
  # A registry the case writes in a form no writer writes (spaces and a CRLF), so that a program that rewrites it, even
  # to the same records, is seen.
  function Get-LooseBytes([string[]]$Records) { $utf8.GetBytes('{"version":0, "hosts": [ ' + ($Records -join ', ') + " ]}`r`n") }
  foreach ($target in 'box', 'host') {
    $tg = $target
    $stackName = Get-StackName $tg
    foreach ($case in @(
        @('another type', $(if ($tg -eq 'box') { $type1 } else { $typeL })),
        @('the same type', $type2)
      )) {
      $held = $case[1]
      $recs = if ($tg -eq 'box') { @((Get-Rec 'swimcoach' 'ready' $idBox $held), $alphaInit, $zeta) } else { @($boxInit, (Get-Rec 'agentspace' 'ready' $idA $held), $zeta) }
      Test-HostCase "${r6}: deploy -InstanceType $type2 removes the instance_type of a record that held $($case[0]) ($held) before cloudformation deploy, changing nothing else ($tg)" (Get-ArgsOf $tg $type2) @{
        Exit = 1; Calls = 'any'
        Check = { param($run, $started)
          Test-Kinds $(if ($tg -eq 'box') { $kindsBoxStop } else { $kindsHostStop })
          $want = Get-Text (Get-Want $tg $(if ($tg -eq 'box') { $idBox } else { $idA }) '')
          if ((Get-HostsAt 'deploy') -cne $want) { 'The record cloudformation deploy was called under is not the target''s with its instance_type removed and nothing else changed, every other record as it was' } } } -Setup {
        Set-Target $recs (Get-Scn $tg @((New-Step (New-StackDoc -StackName $stackName))))
      }
    }
    Test-HostCase "${r6}: deploy -InstanceType $type2 leaves a record that held no instance_type without one, and every other record as it was, before cloudformation deploy ($tg)" (Get-ArgsOf $tg $type2) @{
      Exit = 1; Calls = 'any'
      Check = { param($run, $started)
        # Left as it is, in the loose form the case wrote, or rewritten with the same records: either shows the record without a type.
        $want = Get-Text (Get-Want $tg $(if ($tg -eq 'box') { $idBox } else { $idA }) '')
        $at = Get-HostsAt 'deploy'
        if ($at -cne $want -and $at -cne $script:LooseText) { 'The record cloudformation deploy was called under is not the target''s with no instance_type, every other record as it was' } } } -Setup {
      $r = if ($tg -eq 'box') { @((Get-Rec 'swimcoach' 'ready' $idBox ''), $alphaInit, $zeta) } else { @($boxInit, (Get-Rec 'agentspace' 'ready' $idA ''), $zeta) }
      Set-Target $r (Get-Scn $tg @((New-Step (New-StackDoc -StackName $stackName))))
      $script:LooseBytes = Get-LooseBytes $r
      $script:LooseText = $utf8.GetString($script:LooseBytes)
      Set-HostsFile $script:LooseBytes
    }
    # No write without -InstanceType: the bytes at the deploy call are the case's own, even loosely written.
    Test-HostCase "${r6}: a deploy with no -InstanceType writes nothing before cloudformation deploy, and a cloudformation deploy that fails leaves the registry byte for byte, its type kept ($tg)" (Get-ArgsOf $tg '') @{
      Exit = 1; StderrLines = @($(Get-DepLine $tg), 'An error occurred (ValidationError) when calling the CreateChangeSet operation: no', $deployFailed); Calls = 'any'
      Check = { param($run, $started)
        Test-Kinds $(if ($tg -eq 'box') { @('cloudformation deploy') } else { @('cloudformation describe-stacks', 'cloudformation deploy') })
        if ((Get-HostsAt 'deploy') -cne $script:LooseText) { 'The registry was written before cloudformation deploy, though no -InstanceType was given' }
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $script:LooseBytes)) { 'hosts.json was changed by a deploy that passed no InstanceType and failed at cloudformation deploy' } } } -Setup {
      $script:LooseBytes = Get-LooseBytes $initial
      $script:LooseText = $utf8.GetString($script:LooseBytes)
      Set-Target $initial (Get-Scn $tg @() (New-Step '' "An error occurred (ValidationError) when calling the CreateChangeSet operation: no`n" 255))
      Set-HostsFile $script:LooseBytes
    }
    Test-HostCase "${r6}: a cloudformation deploy that fails with -InstanceType leaves the record without instance_type, and its instance_id and everything else as they were ($tg)" (Get-ArgsOf $tg $type2) @{
      Exit = 1; StderrLines = @($(Get-DepLine $tg), 'An error occurred (ValidationError) when calling the CreateChangeSet operation: no', $deployFailed); Calls = 'any'
      Check = { param($run, $started)
        Test-Kinds $(if ($tg -eq 'box') { @('cloudformation deploy') } else { @('cloudformation describe-stacks', 'cloudformation deploy') })
        & $registryIs (Get-Want $tg $(if ($tg -eq 'box') { $idBox } else { $idA }) '') 'the target''s record without instance_type and otherwise as it was'
        Get-RegistryProblems } } -Setup {
      Set-Target $initial (Get-Scn $tg @() (New-Step '' "An error occurred (ValidationError) when calling the CreateChangeSet operation: no`n" 255))
    }
    # hosts.lock held at step 4: the lock's message, exit 1, no cloudformation deploy.
    Test-HostCase "${r6}: hosts.lock held past its 30 seconds at step 4 gives the lock's message, exit 1, no cloudformation deploy, and the registry as it was ($tg)" (Get-ArgsOf $tg $type2) @{
      Exit = 1; Calls = 'any'
      Check = { param($run, $started)
        Get-LockProblems $run $true
        Test-Kinds $(if ($tg -eq 'box') { @() } else { @('cloudformation describe-stacks') })
        if ($run.Stderr.Contains('Deploying stack')) { 'The Deploying line was printed before step 4''s removal had succeeded' }
        & $registryIs $initial 'as it was'
        Close-HostsLock } } -Setup {
      Set-Target $initial (Get-Scn $tg @((New-Step (New-StackDoc -StackName $stackName))))
      Hold-HostsLock
    } -Timeout 150
  }

  # ======================================================================================================
  # Row: A person's deploy's step 4 with no hosts.json
  # ======================================================================================================
  $r7 = 'deploy-step4-nofile'
  $seedText = '{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}' + "`n"
  $noFile = { Remove-HomePath $fx.HostsFile; Set-AwsScenario (Get-SuccessScn 'box' $type2) }
  foreach ($line in @(@('deploy'), @('deploy', '-Host', 'swimcoach'))) {
    $ln = @($line) + @('-InstanceType', $type2)
    Test-HostCase "${r7}: $($ln -join ' ') with no hosts.json writes the seed document's box record alone, with no instance_type, before cloudformation deploy, then the stack's type with the instance_id" ([string[]]$ln) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $d = @(Get-AwsCalls | Where-Object { $_.a[1] -ceq 'deploy' })
        if ($d.Count -ne 1) { return 'cloudformation deploy was not called once' }
        if ($d[0].hosts -cne $seedText) { 'The registry cloudformation deploy was called under is not the seed document''s box record alone, with no instance_type, compact with one LF' }
        & $registryIs @((New-HostRecord -Project 'swimcoach' -State 'ready' -InstanceId $idNew -InstanceType $type2)) 'the box''s record holding the new instance ID and the type the stack reported'
        Get-RegistryProblems } } -Setup $noFile
    Test-HostCase "${r7}: $($ln -join ' ') with no hosts.json and a cloudformation deploy that fails leaves the seed document's box record alone, with no instance_type" ([string[]]$ln) @{
      Exit = 1; Calls = 'any'
      Check = { param($run, $started)
        if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $utf8.GetBytes($seedText))) { 'hosts.json is not the seed document''s box record alone, with no instance_type' } } } -Setup {
      Remove-HomePath $fx.HostsFile
      Set-AwsScenario @{ 'cloudformation deploy' = @(New-Step '' "An error occurred (ValidationError) when calling the CreateChangeSet operation: no`n" 255) }
    }
  }
  Test-HostCase "${r7}: a plain deploy with no hosts.json writes no hosts.json before cloudformation deploy, then the box's record with the instance_id and the type the stack reports" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $d = @(Get-AwsCalls | Where-Object { $_.a[1] -ceq 'deploy' })
      if ($d.Count -ne 1) { return 'cloudformation deploy was not called once' }
      if ($null -ne $d[0].hosts) { 'hosts.json existed when cloudformation deploy was called, though no -InstanceType was given' }
      & $registryIs @((New-HostRecord -Project 'swimcoach' -State 'ready' -InstanceId $idNew -InstanceType $type2)) 'the box''s record holding the new instance ID and the type the stack reported'
      Get-RegistryProblems } } -Setup $noFile
  Test-HostCase "${r7}: a plain deploy with no hosts.json and a cloudformation deploy that fails writes no hosts.json at all" @('deploy') @{
    Exit = 1; Calls = 'any'
    Check = { param($run, $started) if (Test-Path -LiteralPath $fx.HostsFile) { 'A hosts.json was written by a plain deploy that failed at cloudformation deploy' } } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-AwsScenario @{ 'cloudformation deploy' = @(New-Step '' "An error occurred (ValidationError) when calling the CreateChangeSet operation: no`n" 255) }
  }

  # ======================================================================================================
  # Row: A person's deploy refused while its host's provision.lock is held
  # ======================================================================================================
  $r8 = 'deploy-lock'
  $refusal = { param([string]$P) "agentspace: A provisioning run or deploy for '$P' is already running; the deploy did not run." }
  # The files of the home, by path, size and time, and the folders: a change anywhere in it shows.
  function Get-HomeSnapshot {
    $root = $fx.H.UserProfile.TrimEnd('\')
    $o = [System.Collections.Generic.List[string]]::new()
    foreach ($i in Get-ChildItem -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue) {
      $rel = $i.FullName.Substring($root.Length)
      if ($i.PSIsContainer) { $o.Add("D $rel") } else { $o.Add("F $rel $($i.Length) $($i.LastWriteTimeUtc.Ticks)") }
    }
    $a = $o.ToArray()
    [Array]::Sort($a, [StringComparer]::Ordinal)
    , $a
  }
  function Get-SnapshotProblems {
    $now = Get-HomeSnapshot
    $was = $script:SnapBefore
    if (($now -join "`n") -cne ($was -join "`n")) {
      $first = @(Compare-Object -ReferenceObject $was -DifferenceObject $now | Select-Object -First 1)
      $what = if ($first.Count -gt 0) { ([string]$first[0].InputObject -replace '^(.) (\S+).*$', '$1 $2') } else { 'something' }
      "A file or folder in the home changed ($what)"
    }
    if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $script:HostsBefore)) { 'hosts.json was changed' }
    if (Test-Path -LiteralPath $fx.OperatorDir) { 'An operator folder was made' }
  }
  # One refusal: the lock of $Hold held by the case (or a folder at its path), the registry as the case wrote it
  # (or $Registry's bytes), and nothing changed afterward. -Log makes a provision.log for the held project.
  function Test-Refused {
    param([string]$Name, [string[]]$Arguments, [string]$Hold = '', [string]$FolderAt = '', [string]$Stderr = '', [string]$StderrPattern = '', [int]$Exit = 1,
      [hashtable]$Environment = @{}, [switch]$Log, [switch]$NoPubKey, [switch]$NoParams, [byte[]]$Registry = $null, [string]$StdoutFirst = '')
    $expect = @{ Exit = $Exit; Calls = @(); Check = { param($run, $started)
        Get-SnapshotProblems
        if ($StderrPattern -and $run.Stderr.TrimEnd("`n") -cnotmatch $StderrPattern) { 'Stderr is not the message of the case' }
        if ($StdoutFirst -and -not $run.Stdout.StartsWith($StdoutFirst + "`n")) { 'Stdout does not start with the first line of the help the spec names' } } }
    if (-not $StderrPattern) { $expect.Stderr = $Stderr }
    if ($StdoutFirst) { $expect.StdoutAny = $true }
    Test-HostCase $Name $Arguments $expect -Environment $Environment -Setup {
      if ($null -ne $Registry) { Set-HostsFile $Registry } else { Set-HostsRecords $initial }
      Set-AwsScenario @{}
      if ($NoPubKey) { Remove-HomePath $fx.PubKeyFile }
      if ($NoParams) { Remove-HomePath $fx.ParamsFile }
      if ($Log -and $Hold) {
        New-HomeFolder (Split-Path -Parent (Get-ProvisionLockPath $Hold))
        [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path (Split-Path -Parent (Get-ProvisionLockPath $Hold)) 'provision.log')), "2026-01-01T00:00:00Z run begun`n")
      }
      if ($Environment.ContainsKey('AGENTSPACE_STATE_DIR')) { New-HomeFolder ([string]$Environment['AGENTSPACE_STATE_DIR']) }
      if ($Hold) { Hold-ProvisionLock $Hold }
      if ($FolderAt) { New-HomeFolder (Get-ProvisionLockPath $FolderAt) }
      $script:SnapBefore = Get-HomeSnapshot
      $script:HostsBefore = Get-FileBytesOrNull $fx.HostsFile
    }
  }
  foreach ($case in @(
      @('deploy', @('deploy'), 'swimcoach'),
      @('deploy -InstanceType', @('deploy', '-InstanceType', $type2), 'swimcoach'),
      @('deploy -Host swimcoach', @('deploy', '-Host', 'swimcoach'), 'swimcoach'),
      @('deploy -Host swimcoach -InstanceType', @('deploy', '-Host', 'swimcoach', '-InstanceType', $type2), 'swimcoach'),
      @('deploy -Host SwimCoach -InstanceType', @('deploy', '-Host', 'SwimCoach', '-InstanceType', $type2), 'swimcoach'),
      @('deploy -Quiet', @('deploy', '-Quiet'), 'swimcoach'),
      @('deploy -Host <project>', @('deploy', '-Host', 'agentspace'), 'agentspace'),
      @('deploy -Host <project> -InstanceType', @('deploy', '-Host', 'agentspace', '-InstanceType', $type2), 'agentspace'),
      @('deploy -Host Agentspace -InstanceType', @('deploy', '-Host', 'Agentspace', '-InstanceType', $type2), 'agentspace')
    )) {
    Test-Refused -Name "${r8}: $($case[0]) is refused while the lock of '$($case[2])' is held: exit 1, exactly the message with the project, nothing on stdout, no AWS call, ssh or scp, and no file of the home changed, hosts.json and its instance_type, provision.log and the operator folder included" -Arguments ([string[]]$case[1]) -Hold $case[2] -Stderr ((& $refusal $case[2]) + "`n") -Log:($case[2] -ceq 'agentspace')
  }
  Test-Refused -Name "${r8}: the lock is under the home's .agentspace folder, which AGENTSPACE_STATE_DIR does not move: deploy -InstanceType with the state folder elsewhere is refused" -Arguments @('deploy', '-InstanceType', $type2) -Hold 'swimcoach' -Stderr ((& $refusal 'swimcoach') + "`n") -Environment @{ AGENTSPACE_STATE_DIR = (Join-Path $fx.H.Root 'elsewhere') }
  Test-HostCase "${r8}: the box's lock held does not refuse another host's deploy: deploy -Host <project> goes on to the AWS stand-in" @('deploy', '-Host', 'agentspace', '-InstanceType', $type2) @{
    Exit = 1; Stderr = (Join-Lines @($depHost, $noIdHost)); Calls = @((Get-CallStack $stack), (Get-DeployCall $stack @("InstanceType=$type2")), (Get-CallStack $stack)) } -Setup {
    Set-Target $initial @{ $hostKey = @(New-Step (New-StackDoc -StackName $stack -Status 'UPDATE_COMPLETE')) }
    Hold-ProvisionLock 'swimcoach'
  }
  Test-HostCase "${r8}: another host's lock held does not refuse the box's deploy: deploy goes on to the AWS stand-in" @('deploy', '-InstanceType', $type2) @{
    Exit = 1; Stderr = (Join-Lines @($depBox, $noIdBox)); Calls = 'any'; Check = { param($run, $started) Test-Kinds $kindsBoxStop } } -Setup {
    Set-Target $initial @{ $boxKey = @(New-Step (New-StackDoc -StackName 'agentbox')) }
    Hold-ProvisionLock 'agentspace'
  }
  # The order of refusals: each of these is reported before the lock, with the lock held.
  Test-Refused -Name "${r8}: the order: a usage error comes before the lock (deploy -InstanceType Bad)" -Arguments @('deploy', '-InstanceType', 'Bad') -Hold 'swimcoach' -Stderr ((& $typeMsg 'Bad') + "`n")
  Test-Refused -Name "${r8}: the order: an unknown parameter comes before the lock (deploy -Foo)" -Arguments @('deploy', '-Foo') -Hold 'swimcoach' -Stderr "agentspace: Unknown parameter '-Foo'.`n"
  Test-Refused -Name "${r8}: the order: an -Alias check comes before the lock (deploy -Alias x)" -Arguments @('deploy', '-Alias', 'x') -Hold 'swimcoach' -Stderr "$aliasOnly`n"
  Test-Refused -Name "${r8}: the order: a host check comes before the lock (deploy -All -InstanceType)" -Arguments @('deploy', '-All', '-InstanceType', $type2) -Hold 'swimcoach' -Stderr "$m2`n"
  Test-Refused -Name "${r8}: the order: help comes before the lock, exit 0 (deploy --help)" -Arguments @('deploy', '--help') -Hold 'swimcoach' -Stderr '' -Exit 0 -StdoutFirst 'Usage: agentspace-operator deploy [-Host <project>] [-Platform <os/arch>] [-InstanceType <type>] [-Quiet]'
  Test-Refused -Name "${r8}: the order: the system folder check comes before the lock (SystemRoot unset)" -Arguments @('deploy', '-InstanceType', $type2) -Hold 'swimcoach' -Stderr "$noSystem`n" -Environment @{ SystemRoot = $null }
  Test-Refused -Name "${r8}: the order: a host registry that is not valid is reported before the lock" -Arguments @('deploy', '-InstanceType', $type2) -Hold 'swimcoach' -StderrPattern '^agentspace: The host registry .+ cannot be read: .+\.$' -Registry $utf8.GetBytes('not json {')
  Test-Refused -Name "${r8}: the order: a project with no record is reported before the lock, whichever lock is held (deploy -Host nobody, the lock of 'nobody' held)" -Arguments @('deploy', '-Host', 'nobody') -Hold 'nobody' -Stderr "agentspace: Project 'nobody' has no host; provision -Host nobody makes one.`n"
  Test-Refused -Name "${r8}: the order: a project with no record is reported before the lock (deploy -Host nobody, the box's lock held)" -Arguments @('deploy', '-Host', 'nobody', '-InstanceType', $type2) -Hold 'swimcoach' -Stderr "agentspace: Project 'nobody' has no host; provision -Host nobody makes one.`n"
  Test-Refused -Name "${r8}: the order: a missing id_ed25519.pub is reported after the lock, so the refusal comes first (deploy)" -Arguments @('deploy') -Hold 'swimcoach' -Stderr ((& $refusal 'swimcoach') + "`n") -NoPubKey
  Test-Refused -Name "${r8}: the order: a missing stack-parameters.json is reported after the lock, so the refusal comes first (deploy -InstanceType)" -Arguments @('deploy', '-InstanceType', $type2) -Hold 'swimcoach' -Stderr ((& $refusal 'swimcoach') + "`n") -NoParams -NoPubKey
  # A lock that cannot be taken for another reason, a folder at its path: the message, exit 1, no AWS call.
  foreach ($case in @(
      @('deploy', @('deploy'), 'swimcoach'),
      @('deploy -InstanceType', @('deploy', '-InstanceType', $type2), 'swimcoach'),
      @('deploy -Host <project>', @('deploy', '-Host', 'agentspace'), 'agentspace'),
      @('deploy -Host <project> -InstanceType', @('deploy', '-Host', 'agentspace', '-InstanceType', $type2), 'agentspace')
    )) {
    $lp = [regex]::Escape((Get-ProvisionLockPath $case[2]))
    $lpResolved = [regex]::Escape((Assert-ConformancePath -ConformanceHome $fx.H -Path (Get-ProvisionLockPath $case[2])))
    Test-Refused -Name "${r8}: a folder at the lock's path fails $($case[0]) with Cannot take <the lock's path>: <error>., exit 1, no AWS call, and no file written ($($case[2]))" -Arguments ([string[]]$case[1]) -FolderAt $case[2] -StderrPattern ('(?i)^agentspace: Cannot take (' + $lp + '|' + $lpResolved + '): .+\.$')
  }
  # The lock held from before the stand-in aws's first call to the end of sync, by the project's own lock alone,
  # and free once deploy has exited, the file kept.
  foreach ($case in @(
      @('deploy', 'box', ''),
      @('deploy -Host swimcoach -InstanceType', 'box', $type2),
      @('deploy -Host <project> of a new stack, -InstanceType', 'host', $type2)
    )) {
    $tg = $case[1]; $ty = $case[2]; $proj = Get-ProjectOf $tg
    $argv = if ($case[0] -like '*swimcoach*') { @('deploy', '-Host', 'swimcoach', '-InstanceType', $ty) } else { Get-ArgsOf $tg $ty }
    Test-HostCase "${r8}: $($case[0]): provision.lock of '$proj' alone is held at every call of the stand-in aws, ssh and scp, from the first AWS call to the end of sync, and is free once deploy has exited, the file kept" ([string[]]$argv) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started)
        $aws = @(Get-AwsCalls); $ssh = @(Get-SshCalls); $scp = @(Get-ScpCalls)
        if ($aws.Count -lt 4 -or $ssh.Count -lt 2 -or $scp.Count -lt 2) { return "Too few calls to show the lock held to the end of sync ($($aws.Count) aws, $($ssh.Count) ssh, $($scp.Count) scp)" }
        $notHeld = 0
        foreach ($c in @($aws) + @($ssh) + @($scp)) { if ((@($c.plHeld) -join ',') -cne $proj) { $notHeld++ } }
        if ($notHeld -gt 0) { "$notHeld of $($aws.Count + $ssh.Count + $scp.Count) calls did not find the lock of '$proj' held, and no other" }
        if ($aws[0].a[1] -cne 'describe-stacks' -and $aws[0].a[1] -cne 'deploy') { 'The first AWS call is not deploy''s' }
        $lock = Get-ProvisionLockPath $proj
        if (-not (Test-Path -LiteralPath $lock -PathType Leaf)) { 'The lock file is not there after deploy: it is never deleted' }
        elseif (-not (Test-LockFree $lock)) { 'The lock is still held after deploy has exited' }
        elseif ((Get-Item -LiteralPath $lock).Length -ne 0) { 'The lock file is not empty' } } } -Setup {
      Set-Target $(if ($tg -eq 'box') { $initial } else { $initialCreating }) (Get-SuccessScn $tg $type2 -Create:($tg -eq 'host'))
    }
  }
}
Exit-ConformanceRun

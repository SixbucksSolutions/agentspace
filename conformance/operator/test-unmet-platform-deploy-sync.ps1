#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `deploy` and `sync` on a linux/amd64 host (docs/operator.md, "deploy", "The
# stack", "The platform seam", "sync", "Environment variables", "The operator's folder, one per
# command"; docs/conformance.md, "v0.12's planned cases for the shared and Linux parts", cases W9 and
# W10; D44, D91, D94; E0.12-T04-S02, card #0262, planned by #0248). Every expectation is the spec's
# text: the order of the AWS calls and their arguments, the parameters `deploy` passes, the build's
# line, command and variables.
#
# What it does: each case makes its own throwaway home holding a host registry, the stack parameters
# file, the user's public key, and the stand-ins of conformance/operator/host-provision-common.ps1,
# which log every call with its arguments and the three Go variables the `go` stand-in sees. `go` is
# found through a PATH of the check's own: a stand-in, or none, so no case starts the real `go`. The
# image ID the stand-in's `ssm get-parameter` answers is made up and never a real one.
#
#   W9   `deploy -Host <project> -Platform linux/amd64` reads the platform's public parameter with
#        `ssm get-parameter` once, as a stack is created, and passes `ImageId=<image>` and
#        `InstanceType` last; a stack that exists gets neither; a failed or malformed read fails before
#        `cloudformation deploy`; no line the operator prints holds the image ID; the box's `deploy`
#        and a linux/arm64 host's make no such call and pass no `ImageId`.
#   W10  `sync -Host <project> -Platform linux/amd64` builds the localswim CLI for linux/amd64:
#        `Building the localswim CLI for linux/amd64...`, `go build -trimpath -o <out>
#        ./cmd/localswim` with GOOS=linux, GOARCH=amd64 and CGO_ENABLED=0, `<out>` named
#        localswim-linux-amd64, copied by row 7's scp; a linux/arm64 host's and the box's build is
#        unchanged. A W10 case reports SKIPPED, naming C:\Projects\localswim, when it is absent.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. A failure reason never quotes the program's
# output. The only look at C:\Projects\localswim is one Test-Path on the folder.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-deploy-sync.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$ErrorActionPreference = 'Stop'
$dp = 'operator.md#deploy'
$sy = 'operator.md#sync'
$region = @('--region', 'us-west-2')
$ampStack = Get-PfStack 'agentspace' 'linux/amd64'
$armStack = Get-PfStack 'agentspace' 'linux/arm64'
$amdParameter = '/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id'
$image = $script:PfImage
$imageAnswer = New-HpStep ('{"Parameter":{"Value":"' + $image + '"}}')
$signIn = $script:HpSignIn

# The calls of the AWS CLI, each as one string of its arguments, in order, a deploy's template path
# written <template> so that its one random folder compares.
function Get-AwsLines($Calls) {
  @($Calls | Where-Object { $_['kind'] -eq 'aws' } | ForEach-Object {
      $l = (@($_['a']) -join ' ')
      if ($l -like 'cloudformation deploy *') { $l = $l -replace '--template-file \S+agentspace\.yaml', '--template-file <template>' }
      $l
    })
}
function Get-StackCall([string]$Stack) { (@('cloudformation', 'describe-stacks') + $region + @('--output', 'json', '--stack-name', $Stack)) -join ' ' }
function Get-ParameterCall([string]$Name) { (@('ssm', 'get-parameter') + $region + @('--output', 'json', '--name', $Name)) -join ' ' }
function Get-DeployLine([string]$Stack, [string[]]$Overrides = @()) {
  $a = @('cloudformation', 'deploy') + $region + @('--stack-name', $Stack, '--template-file', '<template>', '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$Stack")
  if ($Overrides.Count -gt 0) { $a += '--parameter-overrides'; $a += $Overrides }
  $a -join ' '
}
function Get-CaKey($H) { (([IO.File]::ReadAllText((Join-Path $H.UserProfile '.agentspace\ca\agentspace_ca.pub')).Trim()) -split '\s+')[0..1] -join ' ' }
function Get-CreateOverrides($H, [string]$Stack, [string[]]$Tail = @()) {
  @("SshPublicKey=$($script:HpKeyLine)", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', "HostName=$Stack", "CaPublicKey=$(Get-CaKey $H)") + $Tail
}

# One deploy case: a home with the records and the AWS answers, the command run, then the AWS calls
# compared with the spec's, in order, the last line of stderr, the registry's bytes (when the case says
# it is left as it was) and the image ID in no printed line. $Want is a scriptblock taking the home.
function Test-DeployCase {
  param(
    [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$Records, [Parameter(Mandatory)][hashtable]$Aws,
    [Parameter(Mandatory)][string[]]$Arguments, [Parameter(Mandatory)][scriptblock]$Want,
    [string]$LastLine = '', [switch]$RegistryUnchanged
  )
  Invoke-HpInHome -Name $Name -Body {
    param($h, $s)
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records $Records)
    Set-HpScenario -H $h -S $s -Scenario @{ aws = $Aws }
    $before = Get-HpFileBytes (Get-HpHostsFile $h)
    $r = Invoke-HpOperator -H $h -S $s -Arguments $Arguments
    $got = @(Get-AwsLines (Get-HpCalls $s))
    $want = @(& $Want $h)
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($got.Count -ne $want.Count) { $p.Add("There were $($got.Count) AWS calls, not the spec's $($want.Count)") }
    else { for ($i = 0; $i -lt $want.Count; $i++) { if ($got[$i] -cne $want[$i]) { $p.Add("AWS call $($i + 1) is not the spec's '$(ConvertTo-ConformanceMasked ($want[$i] -replace [regex]::Escape($script:PfImage), '<image>'))'"); break } } }
    if ($LastLine) {
      $errLines = Get-HpLines $r.Stderr
      $last = if ($errLines.Count -gt 0) { $errLines[$errLines.Count - 1] } else { '' }
      if ($last -cne $LastLine) { $p.Add("The last line of stderr is not '$(ConvertTo-ConformanceMasked $LastLine)'") }
    }
    if ($RegistryUnchanged -and -not (Test-HpSameBytes $before (Get-HpFileBytes (Get-HpHostsFile $h)))) { $p.Add('hosts.json was changed') }
    if ($r.Stdout.Contains($script:PfImage) -or $r.Stderr.Contains($script:PfImage)) { $p.Add('A line the operator printed holds the stand-in''s image ID') }
    Write-HpProblems $Name $p
  }
}

# ============ W9: deploy passes the platform's image ============
$ampRecord = New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -State 'creating'
$armRecord = New-PfRecord -Project 'agentspace' -State 'creating'
$ampCreate = @{ 'cloudformation describe-stacks' = @((New-HpNoStack $ampStack), (New-HpStack $ampStack 'CREATE_COMPLETE' '')); 'ssm get-parameter' = @($imageAnswer) }
foreach ($c in @(
    @{ N = 'without -InstanceType, InstanceType=m8a.xlarge last'; Args = @(); Type = 'm8a.xlarge' },
    @{ N = 'with -InstanceType, its value last'; Args = @('-InstanceType', 'm8a.2xlarge'); Type = 'm8a.2xlarge' })) {
  $c1 = $c
  Test-DeployCase -Name "${dp}: W9: deploy -Host <project> -Platform linux/amd64 for a stack that does not exist reads the platform's public parameter first, then passes the creation parameters, ImageId=<image> after CaPublicKey, and $($c1.N); every call is agentspace-linux-amd64-host-<project>'s and no printed line holds the image ID" `
    -Records @($ampRecord) -Aws $ampCreate -Arguments (@('deploy', '-Host', 'agentspace', '-Platform', 'linux/amd64') + $c1.Args) -Want {
    param($h)
    @((Get-StackCall $ampStack), (Get-ParameterCall $amdParameter),
      (Get-DeployLine $ampStack (Get-CreateOverrides $h $ampStack @("ImageId=$script:PfImage", "InstanceType=$($c1.Type)"))), (Get-StackCall $ampStack))
  } -LastLine "agentspace: Stack '$ampStack' has no InstanceId output."
}
foreach ($c in @(
    @{ N = 'with no parameter at all'; Args = @(); Over = @() },
    @{ N = 'with InstanceType=<type> alone when -InstanceType is given'; Args = @('-InstanceType', 'm8a.2xlarge'); Over = @('InstanceType=m8a.2xlarge') })) {
  $c1 = $c
  Test-DeployCase -Name "${dp}: W9: deploy -Host <project> -Platform linux/amd64 for a stack that exists makes no ssm get-parameter and passes no ImageId, so the image is set at creation and never changed: cloudformation deploy $($c1.N)" `
    -Records @($ampRecord) -Aws @{ 'cloudformation describe-stacks' = @((New-HpStack $ampStack 'UPDATE_COMPLETE' '')) } -Arguments (@('deploy', '-Host', 'agentspace', '-Platform', 'linux/amd64') + $c1.Args) -Want {
    param($h)
    @((Get-StackCall $ampStack), (Get-DeployLine $ampStack $c1.Over), (Get-StackCall $ampStack))
  } -LastLine "agentspace: Stack '$ampStack' has no InstanceId output."
}
$failedRead = @(
  @{ N = 'a failed read prints the AWS CLI''s text'; Step = (New-HpStep '' 'An error occurred (AccessDenied) when calling the GetParameter operation: not allowed' 254); Last = 'agentspace: An error occurred (AccessDenied) when calling the GetParameter operation: not allowed' },
  @{ N = 'an expired sign-in prints exactly the sign-in message'; Step = (New-HpStep '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255); Last = $signIn },
  @{ N = 'an answer with no Parameter.Value'; Step = (New-HpStep '{"Parameter":{}}'); Last = 'agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.' },
  @{ N = 'a Parameter.Value that is not an image ID'; Step = (New-HpStep '{"Parameter":{"Value":"not-an-image"}}'); Last = 'agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.' },
  @{ N = 'a Parameter.Value that is not a string'; Step = (New-HpStep '{"Parameter":{"Value":7}}'); Last = 'agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.' }
)
foreach ($c in $failedRead) {
  $c1 = $c
  Test-DeployCase -Name "${dp}: W9: deploy -Host <project> -Platform linux/amd64: $($c1.N) fails deploy before cloudformation deploy, with exit 1 and the registry as it was (step 3)" `
    -Records @($ampRecord) -Aws @{ 'cloudformation describe-stacks' = @((New-HpNoStack $ampStack)); 'ssm get-parameter' = @($c1.Step) } -Arguments @('deploy', '-Host', 'agentspace', '-Platform', 'linux/amd64') -Want {
    param($h)
    @((Get-StackCall $ampStack), (Get-ParameterCall $amdParameter))
  } -LastLine $c1.Last -RegistryUnchanged
}
Invoke-HpInHome -Name "${dp}: W9: a malformed answer's message holds no part of the answer: the value the stand-in gave is in no line printed" -Body {
  param($h, $s)
  Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @($ampRecord))
  Set-HpScenario -H $h -S $s -Scenario @{ aws = @{ 'cloudformation describe-stacks' = @((New-HpNoStack $ampStack)); 'ssm get-parameter' = @((New-HpStep '{"Parameter":{"Value":"not-an-image"}}')) } }
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('deploy', '-Host', 'agentspace', '-Platform', 'linux/amd64')
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if ($r.Stderr.Contains('not-an-image') -or $r.Stdout.Contains('not-an-image')) { $p.Add('A line the operator printed holds part of the answer') }
  if (-not $r.Stderr.Contains('agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value.')) { $p.Add('Stderr lacks the message for an answer with no usable Parameter.Value') }
  Write-HpProblems "${dp}: W9: a malformed answer's message holds no part of the answer: the value the stand-in gave is in no line printed" $p
}
# The box and a linux/arm64 host: no ssm get-parameter, no ImageId, and the arguments v0.11's.
$armCreate = @{ 'cloudformation describe-stacks' = @((New-HpNoStack $armStack), (New-HpStack $armStack 'CREATE_COMPLETE' '')) }
foreach ($flags in @(@(), @('-Platform', 'linux/arm64'))) {
  $f1 = $flags
  $given = if ($f1.Count) { ' -Platform linux/arm64' } else { '' }
  Test-DeployCase -Name "${dp}: W9: deploy -Host <project>$given for a linux/arm64 host that has no stack makes no ssm get-parameter and passes no ImageId and no InstanceType: the creation parameters of v0.11, byte for byte" `
    -Records @($armRecord) -Aws $armCreate -Arguments (@('deploy', '-Host', 'agentspace') + $f1) -Want {
    param($h)
    @((Get-StackCall $armStack), (Get-DeployLine $armStack (Get-CreateOverrides $h $armStack)), (Get-StackCall $armStack))
  } -LastLine "agentspace: Stack '$armStack' has no InstanceId output."
}
Test-DeployCase -Name "${dp}: W9: the box's deploy makes no ssm get-parameter and passes no ImageId: cloudformation deploy with SshPublicKey and the file's members but InstanceType, and no HostName and no CaPublicKey, as v0.11 made it" `
  -Records @() -Aws @{ 'cloudformation describe-stacks' = @((New-HpStack 'agentbox' 'CREATE_COMPLETE' '')) } -Arguments @('deploy') -Want {
  param($h)
  @((Get-DeployLine 'agentbox' @("SshPublicKey=$($script:HpKeyLine)", 'SubnetId=SUBNET_VALUE', 'VpcId=VPC_VALUE', 'RootVolumeGiB=64')), (Get-StackCall 'agentbox'))
} -LastLine "agentspace: Stack 'agentbox' has no InstanceId output."

# ============ W10: sync builds the localswim CLI for the host's own platform ============
function Test-SyncBuild {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Platform, [string[]]$HostArguments, [Parameter(Mandatory)][string]$Alias, [string]$Record = '')
  Invoke-HpInHome -Name $Name -NeedsLocalswim -Body {
    param($h, $s)
    $records = @(); if ($Record) { $records = @($Record) }
    Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records $records)
    New-HpClaudeMd $h
    New-HpFakeLocalswim -H $h | Out-Null
    Set-HpScenario -H $h -S $s -Scenario @{ aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) } }
    $r = Invoke-HpOperator -H $h -S $s -Arguments (@('sync') + $HostArguments) -WithGo
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $part = $Platform.Replace('/', '-')
    if ($lines -cnotcontains "Building the localswim CLI for $Platform...") { $p.Add("Stderr lacks Building the localswim CLI for $Platform...") }
    $go = @($calls | Where-Object { $_['kind'] -eq 'go' })
    if ($go.Count -ne 1) { $p.Add("go was run $($go.Count) times, not once: no other platform's file is built") }
    else {
      $a = @($go[0]['a'])
      $i = [Array]::IndexOf($a, '-o')
      $out = if ($i -ge 0 -and $i + 1 -lt $a.Count) { [string]$a[$i + 1] } else { '' }
      if (-not ($a.Count -eq 5 -and $a[0] -ceq 'build' -and $a[1] -ceq '-trimpath' -and $a[2] -ceq '-o' -and $a[4] -ceq './cmd/localswim')) { $p.Add('The build is not exactly go build -trimpath -o <out> ./cmd/localswim') }
      $e = $go[0]['goenv']
      if ($e['GOOS'] -cne $Platform.Split('/')[0] -or $e['GOARCH'] -cne $Platform.Split('/')[1] -or $e['CGO_ENABLED'] -cne '0') { $p.Add("The build is not run with GOOS=$($Platform.Split('/')[0]), GOARCH=$($Platform.Split('/')[1]) and CGO_ENABLED=0") }
      if ((Split-Path -Leaf $out) -cne "localswim-$part") { $p.Add("The output file is not named localswim-$part") }
      if ($null -eq (Get-HpOperatorHex $h $out)) { $p.Add('The output file is not in the command''s own folder under %USERPROFILE%\.agentspace\operator\<32 hex>\') }
      $scp = @($calls | Where-Object { $_['kind'] -eq 'scp' })
      $cli = @($scp | Where-Object { @($_['a']) -ccontains "${Alias}:/tmp/localswim" })
      if ($cli.Count -ne 1) { $p.Add('There is no one scp of the localswim CLI to <alias>:/tmp/localswim (row 7)') }
      elseif (-not (Test-HpSamePath ([string]@($cli[0]['a'])[3]) $out)) { $p.Add('Row 7''s scp does not copy the file the build wrote') }
      foreach ($other in @('linux-arm64', 'linux-amd64') | Where-Object { $_ -cne $part }) {
        if (@($calls | Where-Object { @($_['a']) | Where-Object { [string]$_ -like "*localswim-$other*" } }).Count -gt 0) { $p.Add("A call names localswim-$other, another platform's build") }
      }
    }
    Write-HpProblems $Name $p
  }
}
$ampReady = New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -State 'ready' -InstanceId $script:HpHostId
$armReady = New-PfRecord -Project 'agentspace' -State 'ready' -InstanceId $script:HpHostId
Test-SyncBuild -Name "${sy}: W10: sync -Host <project> -Platform linux/amd64 prints Building the localswim CLI for linux/amd64..., runs go build -trimpath -o <out> ./cmd/localswim with GOOS=linux, GOARCH=amd64 and CGO_ENABLED=0, <out> named localswim-linux-amd64 in the command's folder, and row 7's scp copies that file" `
  -Platform 'linux/amd64' -HostArguments @('-Host', 'agentspace', '-Platform', 'linux/amd64') -Alias 'agentspace-host.agentspace.linux-amd64' -Record $ampReady
Test-SyncBuild -Name "${sy}: W10: sync -Host <project> for a linux/arm64 host builds exactly as before v0.12: the linux/arm64 line, GOARCH=arm64 and localswim-linux-arm64" `
  -Platform 'linux/arm64' -HostArguments @('-Host', 'agentspace') -Alias 'agentspace-host.agentspace' -Record $armReady
Test-SyncBuild -Name "${sy}: W10: sync -Host <project> -Platform linux/arm64 builds exactly as before v0.12: the same as no -Platform" `
  -Platform 'linux/arm64' -HostArguments @('-Host', 'agentspace', '-Platform', 'linux/arm64') -Alias 'agentspace-host.agentspace' -Record $armReady
Test-SyncBuild -Name "${sy}: W10: the box's sync builds exactly as before v0.12: the linux/arm64 line, GOARCH=arm64 and localswim-linux-arm64" `
  -Platform 'linux/arm64' -HostArguments @() -Alias 'agentspace' -Record ''

Exit-ConformanceRun

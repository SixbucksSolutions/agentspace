#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code for the checks of the operator's commands per host (docs/operator.md, "Which host a
# command acts on", "Which checks show it"): test-unmet-host-commands*.ps1 and
# test-unmet-host-deploy*.ps1 in conformance/operator/ (E0.07-T04-S02, card #0156). It dot-sources
# helpers.ps1 and so the harness, which starts every program and makes every home. This file starts no
# program under test itself and loads no program's code.
#
# What it gives: one fixture in a throwaway home (stand-ins for `aws`, `ssh` and `scp` that log every
# call and answer from files beside them, the host registry, the stack parameters file, the user's
# public key, an empty PATH folder so that `go` is never found), and a case runner that runs the
# operator once and compares its calls, its stderr, its exit code and what it wrote.
#
# The stand-ins write only beside themselves, under the home's standins folder (docs/conformance.md,
# "The program under test is untrusted": a stand-in's own writes). Every other write and delete that
# this file makes calls Assert-ConformancePath as the path argument of that very call.
. (Join-Path $PSScriptRoot 'helpers.ps1')

# Made-up identifiers. None matches i-0[0-9a-f]{8,16}, and none is a real one.
$script:IdBox = 'i-1234567890abcdef0'
$script:IdA = 'i-fedcba9876543210f'
$script:IdB = 'i-2b2b2b2b2b2b2b2b2'
$script:IdC = 'i-3c3c3c3c3c3c3c3c3'
$script:IdNew = 'i-4d4d4d4d4d4d4d4d4'
$script:Fx = $null

# The volume made up for an instance: the same digits, with the volume prefix.
function Get-VolumeOf([string]$InstanceId) { 'vol-' + $InstanceId.Substring(2) }

# ---- The stand-ins' scripts. Each is a PowerShell 7 script that a .cmd starts, so that it sees its
# arguments as a program would. ----

# The aws stand-in. Each call is logged as one JSON line: its start time, its arguments, and what
# existed when it arrived (the alias files in agentspace.d, the hosts that hold a certificate, the
# host registry's text, the folders in the operator's folder, and for `cloudformation deploy` the
# template's path and the folder it is in). It answers from aws-scenario.json beside it: a key is
# "<service> <operation>", optionally followed by one argument's value (a stack name, an instance ID,
# a volume ID), and its value is a list of steps {out, err, code}; each call takes the next step of
# the first key that matches, and the last step repeats. A call with no key exits 0 with no output, but
# for the three `ssm` calls of a finished patch check, which get the answers of a host that is patched.
$script:AwsStandInScript = @'
$dir = $PSScriptRoot
$a = @($args | ForEach-Object { [string]$_ })
$k = $a[0] + ' ' + $a[1]
$up = $env:USERPROFILE
$entry = [ordered]@{ t = [string][DateTime]::UtcNow.Ticks; a = $a }
$ad = Join-Path $env:AGENTSPACE_SSH_DIR 'agentspace.d'
$entry.aliases = @(if (Test-Path -LiteralPath $ad) { Get-ChildItem -LiteralPath $ad -File | ForEach-Object { $_.Name } })
$entry.certs = @(Get-ChildItem -Path (Join-Path $up '.agentspace\hosts\*\id_ed25519-cert.pub') -ErrorAction SilentlyContinue | ForEach-Object { $_.Directory.Name })
$hf = Join-Path $up '.agentspace\hosts.json'
$entry.hosts = if (Test-Path -LiteralPath $hf -PathType Leaf) { [IO.File]::ReadAllText($hf) } else { $null }
$opd = Join-Path $up '.agentspace\operator'
$entry.ops = @(if (Test-Path -LiteralPath $opd -PathType Container) { Get-ChildItem -LiteralPath $opd -Force | ForEach-Object { $_.Name } })
if ($k -eq 'cloudformation deploy') {
  $i = [array]::IndexOf($a, '--template-file')
  if ($i -ge 0 -and $i + 1 -lt $a.Count) {
    $t = $a[$i + 1]
    $p = Split-Path -Parent $t
    $op = Split-Path -Parent $p
    $entry.tpl = [ordered]@{
      path = $t
      exists = [bool](Test-Path -LiteralPath $t -PathType Leaf)
      name = (Split-Path -Leaf $t)
      parentLeaf = (Split-Path -Leaf $p)
      opLeaf = (Split-Path -Leaf $op)
      marker = [bool](Test-Path -LiteralPath (Join-Path $op 'conformance-marker.txt'))
      siblings = @(if (Test-Path -LiteralPath $op -PathType Container) { Get-ChildItem -LiteralPath $op -Force | ForEach-Object { $_.Name } })
      size = $(if (Test-Path -LiteralPath $t -PathType Leaf) { (Get-Item -LiteralPath $t).Length } else { -1 })
    }
  }
}
[IO.File]::AppendAllText((Join-Path $dir 'aws-args.log'), ((ConvertTo-Json -Compress -Depth 6 -InputObject $entry) + "`n"))
$scn = $null
$f = Join-Path $dir 'aws-scenario.json'
if (Test-Path -LiteralPath $f) { $scn = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f)) }
$cands = [System.Collections.Generic.List[string]]::new()
for ($j = 2; $j -lt $a.Count; $j++) {
  if ($a[$j] -match '^Key=InstanceIds,Values=(.+)$') { $cands.Add($k + ' ' + $Matches[1]) } else { $cands.Add($k + ' ' + $a[$j]) }
}
$cands.Add($k)
$match = $null
if ($null -ne $scn) { foreach ($c in $cands) { if ($null -ne $scn.PSObject.Properties[$c]) { $match = $c; break } } }
if ($null -eq $match) {
  # The defaults of the three Session Manager calls of a patch check that has finished (docs/operator.md,
  # Calls by command; The patch wait): a case that gives a key of its own for one of them keeps its answer.
  $default = switch ($k) {
    'ssm describe-instance-information' { '{"InstanceInformationList":[{"PingStatus":"Online"}]}' }
    'ssm send-command' { '{"Command":{"CommandId":"00000000-0000-0000-0000-000000000000"}}' }
    'ssm get-command-invocation' { '{"Status":"Success","StandardOutputContent":"patched\n"}' }
    default { $null }
  }
  if ($null -ne $default) { [Console]::Out.Write($default); [Console]::Out.Flush() }
  exit 0
}
$steps = @($scn.PSObject.Properties[$match].Value)
$countFile = Join-Path $dir ('aws-count-' + ($match -replace '[^A-Za-z0-9]', '_') + '.txt')
$n = 0
if (Test-Path -LiteralPath $countFile) { $n = [int]([IO.File]::ReadAllText($countFile).Trim()) }
[IO.File]::WriteAllText($countFile, [string]($n + 1))
$step = $steps[[Math]::Min($n, $steps.Count - 1)]
if ($step.err) { [Console]::Error.Write([string]$step.err); [Console]::Error.Flush() }
if ($step.out) { [Console]::Out.Write([string]$step.out); [Console]::Out.Flush() }
exit ([int]$step.code)
'@

# The ssh stand-in. It drains its stdin to ssh-stdin-<n>.bin, logs its arguments and what existed when
# it arrived (the alias files, the hosts that hold a certificate), and answers from ans-ssh.json beside
# it ({out, err, code}, default exit 0). A push (BatchMode=yes among its arguments) is answered as the
# box answers it (docs/operator.md, "What the box does"): the SHA-256 of the normalized content, then
# `zone <zone>` when AGENTSPACE_TIMEZONE names one, and exit 0.
$script:SshStandInScript = @'
$dir = $PSScriptRoot
$a = @($args | ForEach-Object { [string]$_ })
$m = [IO.MemoryStream]::new()
[Console]::OpenStandardInput().CopyTo($m)
$bytes = $m.ToArray()
$n = @(Get-ChildItem -LiteralPath $dir -Filter 'ssh-stdin-*.bin').Count + 1
[IO.File]::WriteAllBytes((Join-Path $dir "ssh-stdin-$n.bin"), $bytes)
$up = $env:USERPROFILE
$entry = [ordered]@{ t = [string][DateTime]::UtcNow.Ticks; a = $a; n = $n }
$ad = Join-Path $env:AGENTSPACE_SSH_DIR 'agentspace.d'
$entry.aliases = @(if (Test-Path -LiteralPath $ad) { Get-ChildItem -LiteralPath $ad -File | ForEach-Object { $_.Name } })
$entry.certs = @(Get-ChildItem -Path (Join-Path $up '.agentspace\hosts\*\id_ed25519-cert.pub') -ErrorAction SilentlyContinue | ForEach-Object { $_.Directory.Name })
$entry.nested = [string]$env:AGENTSPACE_NESTED
[IO.File]::AppendAllText((Join-Path $dir 'ssh-args.log'), ((ConvertTo-Json -Compress -Depth 6 -InputObject $entry) + "`n"))
if ($a -contains 'BatchMode=yes') {
  $text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
  if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
  $lines = @($text -split "`n" | Where-Object { $_ -ne '' })
  $norm = (($lines -join "`n") + "`n")
  $hash = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
  [Console]::Out.Write($hash + '  -' + "`n")
  if ($env:AGENTSPACE_TIMEZONE) { [Console]::Out.Write('zone ' + $env:AGENTSPACE_TIMEZONE + "`n") }
  exit 0
}
$f = Join-Path $dir 'ans-ssh.json'
if (Test-Path -LiteralPath $f) {
  $s = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f))
  if ($null -ne $s.on -and @($s.on) -notcontains $n) { exit 0 }
  if ($s.err) { [Console]::Error.Write([string]$s.err); [Console]::Error.Flush() }
  if ($s.out) { [Console]::Out.Write([string]$s.out); [Console]::Out.Flush() }
  exit ([int]$s.code)
}
exit 0
'@

# The scp stand-in. It logs its arguments and, for each argument that is a local Windows path, whether
# it exists, whether it is a folder, how many files are in it, the leaf of its folder, and whether the
# operator's folder holds the check's marker file (so the path is shown to be under this home's
# operator folder). It answers from ans-scp.json beside it ({out, err, code}, default exit 0).
$script:ScpStandInScript = @'
$dir = $PSScriptRoot
$a = @($args | ForEach-Object { [string]$_ })
$up = $env:USERPROFILE
$entry = [ordered]@{ t = [string][DateTime]::UtcNow.Ticks; a = $a }
$ad = Join-Path $env:AGENTSPACE_SSH_DIR 'agentspace.d'
$entry.aliases = @(if (Test-Path -LiteralPath $ad) { Get-ChildItem -LiteralPath $ad -File | ForEach-Object { $_.Name } })
$entry.certs = @(Get-ChildItem -Path (Join-Path $up '.agentspace\hosts\*\id_ed25519-cert.pub') -ErrorAction SilentlyContinue | ForEach-Object { $_.Directory.Name })
$locals = @()
foreach ($x in $a) {
  if ($x -match '^[A-Za-z]:\\') {
    $isDir = [bool](Test-Path -LiteralPath $x -PathType Container)
    $p = Split-Path -Parent $x
    $op = if ($p) { Split-Path -Parent $p } else { '' }
    $locals += [ordered]@{
      path = $x
      exists = [bool](Test-Path -LiteralPath $x)
      isDir = $isDir
      files = $(if ($isDir) { @(Get-ChildItem -LiteralPath $x -Recurse -File -Force).Count } else { -1 })
      parentLeaf = $(if ($p) { Split-Path -Leaf $p } else { '' })
      opLeaf = $(if ($op) { Split-Path -Leaf $op } else { '' })
      marker = $(if ($op) { [bool](Test-Path -LiteralPath (Join-Path $op 'conformance-marker.txt')) } else { $false })
    }
  }
}
$entry.locals = @($locals)
[IO.File]::AppendAllText((Join-Path $dir 'scp-args.log'), ((ConvertTo-Json -Compress -Depth 6 -InputObject $entry) + "`n"))
$f = Join-Path $dir 'ans-scp.json'
if (Test-Path -LiteralPath $f) {
  $s = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f))
  if ($s.err) { [Console]::Error.Write([string]$s.err); [Console]::Error.Flush() }
  if ($s.out) { [Console]::Out.Write([string]$s.out); [Console]::Out.Flush() }
  exit ([int]$s.code)
}
exit 0
'@

# ---- The fixture ----

# Writes the stand-ins and the files every case starts from into the home, and keeps what the cases
# need in $script:Fx. -Push also writes a fake localswim receipt and a fake DPAPI-protected Bot
# credential for the current user (as test-unmet-push.ps1 does), so that a push is considered.
function Initialize-HostFixture {
  param([Parameter(Mandatory)]$ConformanceHome, [switch]$Push)
  $h = $ConformanceHome
  $pwsh = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
  $mk = {
    param($Name, $Script)
    $cmd = New-OperatorStandIn -ConformanceHome $h -Name $Name -Body @(('"' + $pwsh + '" -NoProfile -File "%~dp0' + $Name + '-standin.ps1" %*'), 'exit /b %ERRORLEVEL%')
    $dir = Split-Path -Parent $cmd
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $dir "$Name-standin.ps1")), $Script)
    $cmd
  }
  $aws = & $mk 'aws' $script:AwsStandInScript
  $ssh = & $mk 'ssh' $script:SshStandInScript
  $scp = & $mk 'scp' $script:ScpStandInScript
  $standins = Split-Path -Parent $aws
  $emptyBin = Join-Path $h.Root 'emptybin'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $emptyBin) | Out-Null
  $agentspace = Join-Path $h.UserProfile '.agentspace'
  $fx = [pscustomobject]@{
    H = $h
    Program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
    Aws = $aws; Ssh = $ssh; Scp = $scp
    Standins = $standins
    Agentspace = $agentspace
    HostsFile = Join-Path $agentspace 'hosts.json'
    HostsLock = Join-Path $agentspace 'hosts.lock'
    HostsDir = Join-Path $agentspace 'hosts'
    OperatorDir = Join-Path $agentspace 'operator'
    ParamsFile = Join-Path $agentspace 'stack-parameters.json'
    PubKeyFile = Join-Path $h.UserProfile '.ssh\id_ed25519.pub'
    SshDir = $h.SshDir
    AliasDir = Join-Path $h.SshDir 'agentspace.d'
    CaDir = Join-Path $agentspace 'ca'
    RetiredFile = Join-Path $agentspace 'instance-id.json'
    Env = @{
      AGENTSPACE_AWS_CMD = $aws; AGENTSPACE_SSH_CMD = $ssh; AGENTSPACE_SCP_CMD = $scp
      PATH = ($emptyBin + ';' + (Join-Path $env:SystemRoot 'System32'))
    }
    Push = [bool]$Push
    Token = 'conformance-fake-token-' + [guid]::NewGuid().ToString('N').Substring(0, 20)
    BaseUrl = 'http://127.0.0.1:18811'
    PubKey = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example'
    SubnetValue = 'SUBNET_VALUE'
    VpcValue = 'VPC_VALUE'
  }
  $script:Fx = $fx
  if ($Push) {
    $lsDir = Join-Path $h.UserProfile 'AppData\Local\localswim-v2'
    $credDir = Join-Path $lsDir 'cred'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $credDir) | Out-Null
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $lsDir 'service.json')), (ConvertTo-Json -Compress -InputObject @{ base = $fx.BaseUrl; credential_directory = $credDir }))
    $secure = ConvertTo-SecureString -String $fx.Token -AsPlainText -Force
    $botText = ConvertTo-Json -Compress -InputObject ([ordered]@{ format = 'localswim-credential'; format_version = 1; role = 'bot'; token_dpapi_hex = (ConvertFrom-SecureString -SecureString $secure) })
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $credDir 'bot.json')), $botText)
  }
  return $fx
}

function Get-Sha256Hex([string]$Text) {
  [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([Text.UTF8Encoding]::new($false).GetBytes($Text))).Replace('-', '').ToLowerInvariant()
}

# ---- The host registry (docs/state.md, "The host registry") ----

# One record as text, its members in the order of the schema's table.
function New-HostRecord {
  param([Parameter(Mandatory)][string]$Project, [string]$State = 'ready', [string]$InstanceId = '', [string]$LastAttempt = '', [string]$Stack = '')
  if (-not $Stack) { $Stack = if ($Project -eq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" } }
  $t = '{"project":"' + $Project + '","stack":"' + $Stack + '","state":"' + $State + '"'
  if ($InstanceId) { $t += ',"instance_id":"' + $InstanceId + '"' }
  if ($LastAttempt) { $t += ',"last_attempt_at":"' + $LastAttempt + '"' }
  $t + '}'
}
function New-BoxRecord([string]$InstanceId = $script:IdBox) { New-HostRecord -Project 'swimcoach' -State 'ready' -InstanceId $InstanceId }

# What a valid registry holds and a writer writes: the records in order, compact, UTF-8, one LF.
function Get-HostsBytes([string[]]$Records) {
  [Text.UTF8Encoding]::new($false).GetBytes('{"version":0,"hosts":[' + ($Records -join ',') + ']}' + "`n")
}
function Set-HostsFile([byte[]]$Bytes) {
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $script:Fx.HostsFile), $Bytes)
}
function Set-HostsRecords([string[]]$Records) { Set-HostsFile (Get-HostsBytes $Records) }
function Get-FileBytesOrNull([string]$Path) { if (Test-Path -LiteralPath $Path -PathType Leaf) { , [IO.File]::ReadAllBytes($Path) } else { $null } }
# The text of one member of one project's record in a registry's text, or $null when the record or the
# member is not there (the member is a record's own, never another record's).
function Get-RecordMember([string]$HostsText, [string]$Project, [string]$Member) {
  if (-not $HostsText) { return $null }
  try { $d = ConvertFrom-Json -InputObject $HostsText -AsHashtable -ErrorAction Stop } catch { return $null }
  foreach ($r in @($d.hosts)) { if ($r.project -ceq $Project) { if ($r.ContainsKey($Member)) { return [string]$r[$Member] } else { return $null } } }
  $null
}
function Test-SameBytes($A, $B) {
  $x = if ($null -eq $A) { '' } else { [Convert]::ToBase64String([byte[]]$A) }
  $y = if ($null -eq $B) { '' } else { [Convert]::ToBase64String([byte[]]$B) }
  $x -ceq $y
}

# ---- The case's state ----

function Remove-HomePath([string]$Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $Path) -Recurse -Force }
}
function New-HomeFolder([string]$Path) {
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $Path) | Out-Null
}

# Puts every case back to its start: the stand-ins' logs and answers, the host registry (the box's
# record with the made-up instance ID), the stack parameters file and the user's public key as
# `deploy` reads them, and no host folder, alias, ssh config, operator folder or retired cache. The CA
# is kept: it belongs to the home.
function Reset-HostCase {
  Close-HostsLock
  Restore-Ca
  $fx = $script:Fx
  foreach ($f in Get-ChildItem -LiteralPath $fx.Standins -File -ErrorAction SilentlyContinue) {
    if ($f.Name -like '*.log' -or $f.Name -like 'aws-count-*' -or $f.Name -like 'ssh-stdin-*' -or $f.Name -like 'ans-*' -or $f.Name -like 'aws-scenario*') {
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $f.FullName) -Force
    }
  }
  # The stand-ins' own call counters are logs too; New-OperatorStandIn's lines go to *-calls.log.
  Remove-HomePath $fx.HostsFile
  Remove-HomePath $fx.HostsLock
  Remove-HomePath $fx.HostsDir
  Remove-HomePath $fx.OperatorDir
  Remove-HomePath $fx.RetiredFile
  Remove-HomePath $fx.AliasDir
  Remove-HomePath (Join-Path $fx.SshDir 'config')
  foreach ($g in Get-ChildItem -LiteralPath $fx.SshDir -File -Force -ErrorAction SilentlyContinue) {
    if ($g.Name -like 'config.agentspace-*') { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $g.FullName) -Force }
  }
  Set-HostsRecords @(New-BoxRecord)
  New-HomeFolder (Split-Path -Parent $fx.PubKeyFile)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.PubKeyFile), $fx.PubKey + "`r`n")
  Set-ParamsFile ('{"version":0,"parameters":{"SubnetId":"' + $fx.SubnetValue + '","VpcId":"' + $fx.VpcValue + '"}}')
  foreach ($f in Get-ChildItem -LiteralPath $fx.Agentspace -File -ErrorAction SilentlyContinue) {
    if ($f.Name -like 'localswim-pushed.json*' -or $f.Name -eq 'projects.json') { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $f.FullName) -Force }
  }
}
function Set-ParamsFile([string]$Text) {
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $script:Fx.ParamsFile), $Text)
}

# Puts the answers of the stand-in aws for the next run: a hashtable of "<service> <operation>[ <value>]"
# to a list of steps. The scenario file is rewritten, and the call counters are forgotten.
function Set-AwsScenario([hashtable]$Scenario) {
  $fx = $script:Fx
  foreach ($f in Get-ChildItem -LiteralPath $fx.Standins -File -Filter 'aws-count-*' -ErrorAction SilentlyContinue) {
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $f.FullName) -Force
  }
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Standins 'aws-scenario.json')), (ConvertTo-Json -Depth 10 -Compress -InputObject $Scenario))
}
function Set-ProgramAnswer([ValidateSet('ssh', 'scp')][string]$Program, [string]$Out = '', [string]$Err = '', [int]$Code = 0, [int[]]$On = @()) {
  $doc = @{ out = $Out; err = $Err; code = $Code }
  if ($PSBoundParameters.ContainsKey("On")) { $doc.on = @($On) }
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path (Join-Path $script:Fx.Standins "ans-$Program.json")), (ConvertTo-Json -Compress -InputObject $doc))
}
function New-Step([string]$Out = '', [string]$Err = '', [int]$Code = 0) { @{ out = $Out; err = $Err; code = $Code } }

# ---- Documents the stand-in aws answers with, in the JSON form the AWS CLI prints ----
function New-InstanceDoc {
  param([string]$Id = $script:IdBox, [string]$State = 'running', [string]$Type = 'm9g.xlarge', [string]$Launch = '2020-01-01T00:00:00+00:00')
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Reservations = @([ordered]@{ Instances = @([ordered]@{
          State = [ordered]@{ Name = $State }; LaunchTime = $Launch; InstanceType = $Type; RootDeviceName = '/dev/sda1'
          BlockDeviceMappings = @([ordered]@{ DeviceName = '/dev/sda1'; Ebs = [ordered]@{ VolumeId = (Get-VolumeOf $Id) } })
        }) }) })
}
function New-VolumesDoc([string]$Id = $script:IdBox, [long]$Size = 20) {
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Volumes = @([ordered]@{ VolumeId = (Get-VolumeOf $Id); Size = $Size; VolumeType = 'gp3' }) })
}
function New-SsmDoc([string]$Id = $script:IdBox, [string]$Ping = 'Online') {
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ InstanceInformationList = @([ordered]@{ InstanceId = $Id; PingStatus = $Ping }) })
}
function New-StackDoc([string]$StackName = 'agentbox', [string]$Id = '', [string]$Status = 'CREATE_COMPLETE') {
  $stack = [ordered]@{ StackName = $StackName; StackStatus = $Status }
  if ($Id) { $stack.Outputs = @([ordered]@{ OutputKey = 'InstanceId'; OutputValue = $Id }) } else { $stack.Outputs = @() }
  ConvertTo-Json -Compress -Depth 10 -InputObject ([ordered]@{ Stacks = @($stack) })
}

# The answers of a healthy host whose instance is $Id: running, launched long ago (so no wait), its
# volume, and Session Manager online. Added to a scenario by `+`.
function Get-HealthyAnswers {
  param([string]$Id = $script:IdBox, [string]$State = 'running', [string]$Type = 'm9g.xlarge', [long]$Size = 20, [string]$Ping = 'Online')
  @{
    "ec2 describe-instances $Id" = @(New-Step (New-InstanceDoc -Id $Id -State $State -Type $Type))
    "ec2 describe-volumes $(Get-VolumeOf $Id)" = @(New-Step (New-VolumesDoc -Id $Id -Size $Size))
    "ssm describe-instance-information $Id" = @(New-Step (New-SsmDoc -Id $Id -Ping $Ping))
  }
}

# ---- Reading what the stand-ins logged ----
function Read-JsonLog([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @() }
  @(Get-Content -LiteralPath $Path | Where-Object { $_.Trim() -ne '' } | ForEach-Object {
      $o = ConvertFrom-Json -InputObject $_ -AsHashtable
      $o.a = @($o.a | ForEach-Object { [string]$_ })
      $o.Text = $o.a -join ' '
      [pscustomobject]$o
    })
}
function Get-AwsCalls { Read-JsonLog (Join-Path $script:Fx.Standins 'aws-args.log') }
function Get-SshCalls { Read-JsonLog (Join-Path $script:Fx.Standins 'ssh-args.log') }
function Get-ScpCalls { Read-JsonLog (Join-Path $script:Fx.Standins 'scp-args.log') }

# One wanted argument matches the argument the program got: `re:` starts a regular expression, matched
# case-sensitively; anything else is compared exactly.
function Test-ArgumentMatch([string]$Want, [string]$Got) {
  if ($Want.StartsWith('re:')) { return $Got -cmatch $Want.Substring(3) }
  $Got -ceq $Want
}
# A wanted call is a list of arguments, or one string split at single spaces.
function Test-CallMatch($Want, [string[]]$Got) {
  $w = if ($Want -is [string]) { @($Want -split ' ') } else { @($Want) }
  if ($w.Count -ne $Got.Count) { return $false }
  for ($i = 0; $i -lt $w.Count; $i++) { if (-not (Test-ArgumentMatch ([string]$w[$i]) $Got[$i])) { return $false } }
  $true
}
# Adds one problem when the calls differ from the wanted ones, naming the first that does not match.
function Add-CallProblem($Problems, [string]$What, $Calls, $Want) {
  $want = @($Want | Where-Object { $null -ne $_ })
  $calls = @($Calls | Where-Object { $null -ne $_ })
  if ($calls.Count -ne $want.Count) { $Problems.Add("$What was called $($calls.Count) times, not $($want.Count)") }
  for ($i = 0; $i -lt [Math]::Min($calls.Count, $want.Count); $i++) {
    if (-not (Test-CallMatch $want[$i] $calls[$i].a)) {
      $w = if ($want[$i] -is [string]) { $want[$i] } else { @($want[$i]) -join ' ' }
      $Problems.Add("$What call $($i + 1) is not '$($w -replace '\s+', ' ')'")
      break
    }
  }
}

# ---- The files a signing leaves ----
function Get-HostFiles([string]$Project) {
  $fx = $script:Fx
  $dir = Join-Path $fx.HostsDir $Project
  [pscustomobject]@{
    Dir = $dir
    Key = Join-Path $dir 'id_ed25519'
    Pub = Join-Path $dir 'id_ed25519.pub'
    Cert = Join-Path $dir 'id_ed25519-cert.pub'
    Alias = Join-Path $fx.AliasDir "agentspace-host.$Project"
    Stamp = Join-Path $dir 'localswim-pushed.json'
    Log = Join-Path $dir 'provision.log'
  }
}
# What an operator command that signs for a host leaves: the key, the certificate, the alias file and
# the Include line. Returns a list of problems; none means all four are there.
function Get-SignProblems([string]$Project) {
  $f = Get-HostFiles $Project
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in @(@('the operator''s key', $f.Key), @('the certificate', $f.Cert), @('the alias file', $f.Alias))) {
    if (-not (Test-Path -LiteralPath $x[1] -PathType Leaf)) { $p.Add("$($x[0]) for '$Project' was not written") }
  }
  $cfg = Join-Path $script:Fx.SshDir 'config'
  if (-not (Test-Path -LiteralPath $cfg -PathType Leaf)) { $p.Add('The ssh config holds no Include line') }
  elseif (-not ([IO.File]::ReadAllText($cfg) -match '(?im)^\s*Include\s+agentspace\.d/\*\s*$')) { $p.Add('The ssh config holds no Include line') }
  if ($p.Count -eq 0) {
    $kg = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
    $out = (& $kg -L -f $f.Cert 2>&1 | Out-String)
    if ($out -notmatch ('Key ID: "operator-' + [regex]::Escape($Project) + '"')) { $p.Add("The certificate's key ID is not operator-$Project") }
    if ($out -notmatch 'Type: ssh-ed25519-cert-v01@openssh\.com user certificate') { $p.Add('The certificate is not an ed25519 user certificate') }
  }
  return $p.ToArray()
}
# True when nothing was written for any host's operator access: no hosts folder, no alias, no ssh config.
function Get-NothingSignedProblems {
  $fx = $script:Fx
  $p = [System.Collections.Generic.List[string]]::new()
  if (Test-Path -LiteralPath $fx.HostsDir) { $p.Add('A hosts folder was made') }
  if (Test-Path -LiteralPath $fx.AliasDir) {
    if (@(Get-ChildItem -LiteralPath $fx.AliasDir -Force -ErrorAction SilentlyContinue).Count -gt 0) { $p.Add('An alias file was written') }
  }
  if (Test-Path -LiteralPath (Join-Path $fx.SshDir 'config')) { $p.Add('An ssh config was written') }
  return $p.ToArray()
}

# Moves the CA's folder aside and back, so that a case can run with no CA and then restore it with its
# access rules (a move keeps them; a copy would not).
function Hide-Ca {
  $fx = $script:Fx
  $away = Join-Path $fx.H.Root 'ca-away'
  Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.CaDir) -Destination (Assert-ConformancePath -ConformanceHome $fx.H -Path $away)
}
function Restore-Ca {
  $fx = $script:Fx
  $away = Join-Path $fx.H.Root 'ca-away'
  if (Test-Path -LiteralPath $away) {
    Move-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $away) -Destination (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.CaDir)
  }
}

# ---- Running a case ----

# Runs the operator once with the stand-ins and returns the harness's result with Started (UTC) and
# Elapsed (seconds) added. Nothing is reset first and nothing is checked.
function Invoke-HostRun {
  param([Parameter(Mandatory)][AllowEmptyString()][string[]]$Arguments, [hashtable]$Environment = @{}, [int]$Timeout = 120)
  $fx = $script:Fx
  $started = [DateTime]::UtcNow
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $fx.H -Arguments $Arguments -Environment ($fx.Env + $Environment) -TimeoutSeconds $Timeout
  $r | Add-Member -NotePropertyName Started -NotePropertyValue $started -Force
  $r | Add-Member -NotePropertyName Elapsed -NotePropertyValue $sw.Elapsed.TotalSeconds -Force
  $r
}
# Forgets what the stand-ins logged (the calls, the counters, the stdin files), keeping their answers.
function Clear-StandinLogs {
  $fx = $script:Fx
  foreach ($f in Get-ChildItem -LiteralPath $fx.Standins -File -ErrorAction SilentlyContinue) {
    if ($f.Name -like '*.log' -or $f.Name -like 'aws-count-*' -or $f.Name -like 'ssh-stdin-*') {
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $f.FullName) -Force
    }
  }
}

# Holds hosts.lock the way a registry writer does, open with no sharing, until Close-HostsLock.
function Hold-HostsLock {
  $fx = $script:Fx
  $script:LockStream = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.HostsLock), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
}
function Close-HostsLock {
  if ($null -ne $script:LockStream) { $script:LockStream.Dispose(); $script:LockStream = $null }
}

# The AWS calls of Calls by command, as one string each (their arguments separated by one space).
function Get-CallStack([string]$Stack) { "cloudformation describe-stacks --region us-west-2 --output json --stack-name $Stack" }
function Get-CallDescribe([string]$Id) { "ec2 describe-instances --region us-west-2 --output json --instance-ids $Id" }
function Get-CallVolumes([string]$Id) { "ec2 describe-volumes --region us-west-2 --output json --volume-ids $(Get-VolumeOf $Id)" }
function Get-CallSsm([string]$Id) { "ssm describe-instance-information --region us-west-2 --output json --filters Key=InstanceIds,Values=$Id" }
# The three calls of `sync`'s patch wait (docs/operator.md, Calls by command and The patch wait), in order, for the
# host whose instance is $Id: the Session Manager read, the send with the comment `agentspace sync: patch check`
# (its question is the box's own wording, so its value is `commands=` and then anything), and the poll of the
# command the send answered, which with the stand-in's default answer is the command ID of zeros. A wait that
# follows a sync's install is these three and no `ec2 describe-instances` of its own.
$script:PatchWaitCommandId = '00000000-0000-0000-0000-000000000000'
function Get-CallPatchSend([string]$Id) { @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $Id, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace sync: patch check', '--parameters', 're:^commands=') }
function Get-CallPatchPoll([string]$Id, [string]$CommandId = $script:PatchWaitCommandId) { "ssm get-command-invocation --region us-west-2 --output json --command-id $CommandId --instance-id $Id" }
function Get-PatchWaitCalls([string]$Id, [string]$CommandId = $script:PatchWaitCommandId) { @((Get-CallSsm $Id), (Get-CallPatchSend $Id), (Get-CallPatchPoll $Id $CommandId)) }
function Get-CallStop([string]$Id) { "ec2 stop-instances --region us-west-2 --output json --instance-ids $Id" }
function Get-CallStart([string]$Id) { "ec2 start-instances --region us-west-2 --output json --instance-ids $Id" }
function Get-CallModify([string]$Id, [long]$Size) { "ec2 modify-volume --region us-west-2 --output json --volume-id $(Get-VolumeOf $Id) --size $Size" }
function Get-StatusLines([string]$Id, [string]$State = 'running', [string]$Type = 'm9g.xlarge', [long]$Size = 20, [string]$Ssm = 'online') {
  Join-Lines @("Instance  $Id", "State     $State", "Type      $Type", "Disk      $Size GiB gp3 ($(Get-VolumeOf $Id))", "SSM       $Ssm")
}

# Runs the operator once with the stand-ins and reports one result. $Expect holds:
#   Exit       The exit code (required).
#   Stdout     The exact stdout (default empty).
#   Stderr     The exact stderr; or StderrLines, lines that each MUST be a whole line of it, in order,
#              as a subsequence; and StderrNot, text that MUST NOT appear in it.
#   Calls      The aws calls, in order (see Test-CallMatch); SshCalls and ScpCalls likewise. A call count
#              is checked only when the key is given, except that ssh and scp default to none.
#   Check      A scriptblock taking the run and the start time, returning problems (strings).
# $Setup runs after Reset-HostCase and before the operator starts.
function Test-HostCase {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][AllowEmptyString()][string[]]$Arguments,
    [Parameter(Mandatory)][hashtable]$Expect,
    [scriptblock]$Setup = {},
    [hashtable]$Environment = @{},
    [int]$Timeout = 120
  )
  $fx = $script:Fx
  try {
    Reset-HostCase
    & $Setup
    $r = Invoke-HostRun -Arguments $Arguments -Environment $Environment -Timeout $Timeout
    $started = $r.Started
    $p = [System.Collections.Generic.List[string]]::new()
    if ($r.TimedOut) { $p.Add('It timed out') }
    elseif ($r.ExitCode -ne $Expect.Exit) { $p.Add("Exit $($r.ExitCode), not $($Expect.Exit)") }
    $wantOut = if ($Expect.ContainsKey('Stdout')) { $Expect.Stdout } else { '' }
    if (-not $Expect.StdoutAny -and $r.Stdout -cne $wantOut) { $p.Add("Stdout is not the expected $($wantOut.Length) characters (it has $($r.Stdout.Length))") }
    if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $p.Add('Stderr shows a runtime error record') }
    if ($r.Stderr.Contains("`r")) { $p.Add('Stderr holds a carriage return') }
    if ($Expect.ContainsKey('Stderr')) {
      $x = Compare-Text 'Stderr' $r.Stderr $Expect.Stderr
      if ($x) { $p.Add($x) }
    }
    if ($Expect.ContainsKey('StderrLines')) {
      $lines = @($r.Stderr -split "`n" | Where-Object { $_ -ne '' })
      $at = 0
      foreach ($want in @($Expect.StderrLines)) {
        $found = $false
        while ($at -lt $lines.Count) {
          $ok = if ($want.StartsWith('re:')) { $lines[$at] -cmatch $want.Substring(3) } else { $lines[$at] -ceq $want }
          $at++
          if ($ok) { $found = $true; break }
        }
        if (-not $found) { $p.Add("Stderr lacks the line '$($want -replace '\s+', ' ')' (or it is out of order)"); break }
      }
    }
    if ($Expect.ContainsKey('StderrNot')) {
      foreach ($bad in @($Expect.StderrNot)) { if ($r.Stderr.Contains($bad)) { $p.Add("Stderr holds '$bad'") } }
    }
    if ($Expect.ContainsKey('Calls') -and -not ($Expect.Calls -is [string] -and $Expect.Calls -eq 'any')) { Add-CallProblem $p 'The AWS CLI' (Get-AwsCalls) $Expect.Calls }
    # ssh and scp default to no call; the value 'any' leaves the calls unchecked here (a case that reads them itself).
    foreach ($prog in @(@('SshCalls', 'ssh'), @('ScpCalls', 'scp'))) {
      $wantSshScp = @()
      if ($Expect.ContainsKey($prog[0])) { $wantSshScp = $Expect[$prog[0]] }
      if ($wantSshScp -is [string] -and $wantSshScp -eq 'any') { continue }
      Add-CallProblem $p $prog[1] $(if ($prog[1] -eq 'ssh') { Get-SshCalls } else { Get-ScpCalls }) $wantSshScp
    }
    if ($Expect.ContainsKey('Check')) { foreach ($m in @(& $Expect.Check $r $started)) { if ($m) { $p.Add([string]$m) } } }
    if ($p.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
    else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($p -join '; ') }
  } catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  }
}

# A failure reason names the line that differs and quotes the spec's text, never the program's.
function Compare-Text([string]$What, [string]$Got, [string]$Want) {
  if ($Got -ceq $Want) { return $null }
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
function Join-Lines([string[]]$Lines) { ($Lines -join "`n") + "`n" }

# Runs the setup the whole file needs once: the throwaway home, the AWS gate and the fixture. Calls
# $Body with the fixture when the gate is open and the program is named; otherwise writes the one
# SKIPPED or FAIL result and returns.
function Invoke-HostFile {
  param([Parameter(Mandatory)][string]$FileName, [Parameter(Mandatory)][scriptblock]$Body, [switch]$Push)
  $h = New-ConformanceHome
  try {
    $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
    $gate = Test-AwsUnreachable -ConformanceHome $h
    $seamName = "operator.md#the-test-seam: $FileName runs the stand-ins"
    if (-not $gate.Unreachable) {
      Write-ConformanceResult -Name $seamName -Status SKIPPED -Reason $gate.Reason
    } elseif (-not $program) {
      Write-ConformanceResult -Name $seamName -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'
    } elseif (-not $h.CaReady) {
      Write-ConformanceResult -Name $seamName -Status SKIPPED -Reason "The throwaway home has no CA: $($h.CaReason)"
    } else {
      $fx = Initialize-HostFixture -ConformanceHome $h -Push:$Push
      & $Body $fx
    }
  } catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name "${FileName}: The check ran to its end without an unexpected error" -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    Close-HostsLock
    Remove-ConformanceHome -ConformanceHome $h
  }
}

# Joins scenarios: a key given by a later one replaces the same key of an earlier one.
function Merge-Scenario {
  $o = @{}
  foreach ($s in $args) { foreach ($k in $s.Keys) { $o[$k] = $s[$k] } }
  $o
}

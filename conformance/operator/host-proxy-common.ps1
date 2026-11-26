#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code of the test-unmet-host-proxy*.ps1 and test-unmet-host-alias*.ps1 checks (docs/operator.md,
# "From the ProxyCommand", "The host registry, as the operator uses it", "How the operator reaches a
# host", "The token push"; E0.07-T04-S02). It dot-sources helpers.ps1, so every program still starts
# through the harness, in a throwaway home, and this file starts no program of its own.
#
# What is here: made-up IDs and records, writers of hosts.json, leases.json and a stand-in stack's
# answer (each call Assert-ConformancePath at the write), the stand-in aws that answers per stack and
# logs every call's full arguments, the push's fixture and its stand-in ssh, and readers of what the
# stand-ins logged. A check never reports a program's output, which can hold an instance ID.

. (Join-Path $PSScriptRoot 'helpers.ps1')

$ErrorActionPreference = 'Stop'

# Made-up instance IDs. None matches i-0[0-9a-f]{8,16}, and none is a real instance.
$HpIdBox = 'i-1234567890abcdef0'
$HpIdOther = 'i-fedcba9876543210f'
$HpIdOther2 = 'i-bbbb2222cccc3333d'
$HpIdThird = 'i-abcdef1234567890a'
$HpIdFailed = 'i-aaaa1111bbbb2222c'
$HpIdSpare = 'i-cccc3333dddd4444e'
$HpCouldNotReach = 'agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.'
$HpSeamSec = 'operator.md#the-test-seam'

# ---- Problems, as test-unmet-proxy-steps.ps1 reports them ----
function New-HpProblems { , [System.Collections.Generic.List[string]]::new() }
function Write-HpProblems([string]$Name, $Problems) {
  if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($Problems -join '; ')) }
}

# ---- Files ----
function Set-HpFile($H, [string]$Path, [string]$Text) {
  $dir = Split-Path -Parent $Path
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $dir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $H -Path $Path), [Text.UTF8Encoding]::new($false).GetBytes($Text))
}
function Set-HpBytes($H, [string]$Path, [byte[]]$Bytes) {
  $dir = Split-Path -Parent $Path
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $dir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $H -Path $Path), $Bytes)
}
function Remove-HpPath($H, [string]$Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $H -Path $Path) -Recurse -Force }
}
function Get-HpBytesHash([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return 'absent' }
  [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($Path))).Replace('-', '')
}
function Get-HpText([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  [Text.UTF8Encoding]::new($false).GetString([IO.File]::ReadAllBytes($Path))
}
function Get-HpRegistryPath($H) { Join-Path $H.UserProfile '.agentspace\hosts.json' }
function Get-HpLeasesPath($H) { Join-Path $H.StateDir 'leases.json' }
function Get-HpHostDir($H, [string]$Project) { Join-Path $H.UserProfile ".agentspace\hosts\$Project" }

# ---- Records ----
# A ready record holds instance_type m9g.xlarge, D65's m, unless -NoInstanceType is given, so that a
# lease CLI create placed on it answers ready as before v0.09 (D67; architect, 2026-11-24, for
# v0.09-T04, pin P56, as pin P3 gave the other helpers).
function New-HpHostRecord([string]$Project, [string]$State = 'ready', [string]$InstanceId = '', [string]$LastAttempt = '', [switch]$NoInstanceType) {
  $stack = if ($Project -eq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" }
  $t = '{"project":"' + $Project + '","stack":"' + $stack + '","state":"' + $State + '"'
  if ($InstanceId) { $t += ',"instance_id":"' + $InstanceId + '"' }
  if ($State -ceq 'ready' -and -not $NoInstanceType) { $t += ',"instance_type":"m9g.xlarge"' }
  if ($LastAttempt) { $t += ',"last_attempt_at":"' + $LastAttempt + '"' }
  $t + '}'
}
function New-HpRegistryText([string[]]$Records) { '{"version":0,"hosts":[' + ($Records -join ',') + ']}' + "`n" }
function Set-HpRegistry($H, [string[]]$Records) {
  Remove-HpPath $H (Get-HpRegistryPath $H)
  Set-HpFile $H (Get-HpRegistryPath $H) (New-HpRegistryText $Records)
}
# A ready lease's record, as the lease state file holds it (state.md, "The lease state file").
function New-HpLeaseRecord([string]$Id, [string]$Project, [string]$Alias, [string]$Expires = '2099-01-01T00:00:00Z') {
  '{"id":"' + $Id + '","project":"' + $Project + '","state":"ready","platform":"linux/arm64","profile":"agent-linux@3","size":"m","ttl_seconds":28800,"created_at":"2026-01-01T00:00:00Z","expires_at":"' + $Expires + '","ssh":{"host_alias":"' + $Alias + '","user":"ubuntu"},"driver":"ec2"}'
}
function Set-HpLeases($H, [string[]]$Records) {
  Remove-HpPath $H (Get-HpLeasesPath $H)
  Set-HpFile $H (Get-HpLeasesPath $H) ('{"version":0,"leases":[' + ($Records -join ',') + ']}' + "`n")
}

# ---- The stand-ins ----
# The stand-in aws logs every call's full arguments to aws-full.log beside itself, answers
# `describe-instances` with a running instance launched long ago (so `up` does not wait), starts the
# tunnel by printing a marker to stdout and exiting 0, and answers `describe-stacks --stack-name <n>`
# from stack-<n>.json beside it, or fails as the CLI does for a stack that does not exist.
function New-HpStandIns($H) {
  $aws = New-OperatorStandIn -ConformanceHome $H -Name 'aws' -Body @(
    'set "D=%~dp0"',
    'echo %* >>"%D%aws-full.log"',
    'if /i "%~1"=="ssm" if /i "%~2"=="start-session" goto tunnel',
    'if /i "%~1"=="ec2" if /i "%~2"=="describe-instances" goto describe',
    'if /i "%~1"=="cloudformation" if /i "%~2"=="describe-stacks" goto stacks',
    'exit /b 0',
    ':tunnel',
    'echo tunnel-says-hello',
    'exit /b 0',
    ':describe',
    'echo {"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}',
    'exit /b 0',
    ':stacks',
    'set "NAME="',
    ':loop',
    'if "%~1"=="" goto found',
    'if /i "%~1"=="--stack-name" set "NAME=%~2"',
    'shift',
    'goto loop',
    ':found',
    'if exist "%D%stack-%NAME%.json" (type "%D%stack-%NAME%.json" & exit /b 0)',
    'echo An error occurred (ValidationError) when calling the DescribeStacks operation 1>&2',
    'exit /b 254')
  $trip = Get-OperatorTripwireEnvironment -ConformanceHome $H
  [pscustomobject]@{ Aws = $aws; Ssh = $trip['AGENTSPACE_SSH_CMD']; Scp = $trip['AGENTSPACE_SCP_CMD']; Dir = (Split-Path -Parent $aws) }
}
# The answer of `describe-stacks` for a stack: its InstanceId output (empty $InstanceId leaves the
# output out, so the stack has no InstanceId output).
function Set-HpStack($H, $S, [string]$Stack, [string]$InstanceId) {
  $outs = if ($InstanceId) { '[{"OutputKey":"InstanceId","OutputValue":"' + $InstanceId + '"}]' } else { '[{"OutputKey":"Unrelated","OutputValue":"x"}]' }
  Set-HpFile $H (Join-Path $S.Dir "stack-$Stack.json") ('{"Stacks":[{"StackName":"' + $Stack + '","StackStatus":"CREATE_COMPLETE","Outputs":' + $outs + '}]}')
}
function Clear-HpStacks($H, $S) {
  foreach ($f in Get-ChildItem -LiteralPath $S.Dir -Filter 'stack-*.json' -File -ErrorAction SilentlyContinue) { Remove-HpPath $H $f.FullName }
}
function Clear-HpLogs($H, $S) {
  foreach ($f in Get-ChildItem -LiteralPath $S.Dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like '*.log' -or $_.Name -like '*-stdin-*.bin' }) { Remove-HpPath $H $f.FullName }
}
function Get-HpLines([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @() }
  @(Get-Content -LiteralPath $Path | Where-Object { $_.Trim() -ne '' } | ForEach-Object { $_.Trim() })
}
# What the stand-in aws was asked: the stacks named, the instance IDs described, and the tunnel's targets.
function Get-HpAsked($S) {
  $full = @(Get-HpLines (Join-Path $S.Dir 'aws-full.log'))
  $stacks = [System.Collections.Generic.List[string]]::new()
  $describes = [System.Collections.Generic.List[string]]::new()
  $tunnels = [System.Collections.Generic.List[string]]::new()
  foreach ($l in $full) {
    if ($l -match '^cloudformation\s+describe-stacks\b' -and $l -match '--stack-name\s+(\S+)') { $stacks.Add($Matches[1]) }
    elseif ($l -match '^ec2\s+describe-instances\b' -and $l -match '--instance-ids\s+(\S+)') { $describes.Add($Matches[1]) }
    elseif ($l -match '^ssm\s+start-session\b' -and $l -match '--target\s+(\S+)') { $tunnels.Add($Matches[1]) }
  }
  [pscustomobject]@{ Raw = $full; Stacks = $stacks.ToArray(); Describes = $describes.ToArray(); Tunnels = $tunnels.ToArray() }
}
function Get-HpSshCallCount($S) { Get-StandInCallCount -Log (Join-Path $S.Dir 'ssh-tripwire-calls.log') }

# Runs the ProxyCommand under the harness, with the stand-ins in the environment. AGENTSPACE_NESTED is
# set unless -Push, so a run pushes nothing and needs no ssh.
function Invoke-HpProxy($H, $S, [string[]]$Arguments = @(), [hashtable]$Extra = @{}, [switch]$Push, [int]$Timeout = 120) {
  $e = @{ AGENTSPACE_AWS_CMD = $S.Aws; AGENTSPACE_SSH_CMD = $S.Ssh; AGENTSPACE_SCP_CMD = $S.Scp }
  if (-not $Push) { $e['AGENTSPACE_NESTED'] = '1' }
  foreach ($k in $Extra.Keys) { $e[$k] = $Extra[$k] }
  Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $H -Arguments $Arguments -Environment $e -TimeoutSeconds $Timeout
}
function Invoke-HpOperator($H, $S, [string[]]$Arguments, [hashtable]$Extra = @{}, [int]$Timeout = 180) {
  $e = @{ AGENTSPACE_AWS_CMD = $S.Aws; AGENTSPACE_SSH_CMD = $S.Ssh; AGENTSPACE_SCP_CMD = $S.Scp }
  foreach ($k in $Extra.Keys) { $e[$k] = $Extra[$k] }
  Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $H -Arguments $Arguments -Environment $e -TimeoutSeconds $Timeout
}

# The problems of a ProxyCommand run that reached a host: exit 0 (the stand-in tunnel's), stdout only
# the tunnel's, one tunnel for $Want, every instance described $Want, no stack asked, no ssh or scp.
function Get-HpReachedProblems($Run, $S, [string]$Want, [switch]$StacksAllowed) {
  $p = New-HpProblems
  $a = Get-HpAsked $S
  if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
  if ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not the tunnel's 0") }
  if ($Run.Stdout.Trim() -cne 'tunnel-says-hello') { $p.Add('Stdout is not exactly the tunnel''s output') }
  if ($a.Raw.Count -eq 0) { $p.Add('The stand-in aws was never called (seam not met)') }
  if ($a.Tunnels.Count -ne 1) { $p.Add("The tunnel was started $($a.Tunnels.Count) times, not once") }
  elseif ($a.Tunnels[0] -cne $Want) { $p.Add('The tunnel is for another instance than the host''s') }
  if (@($a.Describes | Where-Object { $_ -cne $Want }).Count -gt 0) { $p.Add('An instance other than the host''s was asked about') }
  if ($a.Describes.Count -eq 0) { $p.Add('No instance was asked about') }
  if (-not $StacksAllowed -and $a.Stacks.Count -gt 0) { $p.Add('A stack was asked for, though the host''s record holds the instance ID') }
  if ((Get-HpSshCallCount $S) -gt 0 -or (Get-StandInCallCount -Log (Join-Path $S.Dir 'scp-tripwire-calls.log')) -gt 0) { $p.Add('ssh or scp was started') }
  , $p
}
# The problems of a run the ProxyCommand refused: exit 1, nothing on stdout, no tunnel, and the
# caller's "Could not reach the host" line last on stderr.
function Get-HpRefusedProblems($Run, $S) {
  $p = New-HpProblems
  $a = Get-HpAsked $S
  if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
  if ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
  if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
  if ($a.Tunnels.Count -gt 0) { $p.Add('A tunnel was started') }
  $lines = @($Run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
  if ($lines.Count -eq 0 -or $lines[-1].Trim() -cne $HpCouldNotReach) { $p.Add('The last line on stderr is not the "Could not reach the host" message') }
  if ($Run.Stderr -match 'CategoryInfo|At line:|Exception') { $p.Add('Stderr shows a runtime error record') }
  , $p
}

# ---- The token push's fixture (a made-up token, a receipt and a DPAPI credential, as test-unmet-push.ps1 writes them) ----
$HpBaseUrl = 'http://127.0.0.1:18811'
function New-HpPushFixture($H) {
  $token = 'conformance-fake-token-' + [guid]::NewGuid().ToString('N').Substring(0, 20)
  $lsDir = Join-Path $H.UserProfile 'AppData\Local\localswim-v2'
  $credDir = Join-Path $lsDir 'cred'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $credDir) | Out-Null
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $H -Path (Join-Path $lsDir 'service.json')), (ConvertTo-Json -Compress -InputObject @{ base = $HpBaseUrl; credential_directory = $credDir }))
  $hex = ConvertFrom-SecureString -SecureString (ConvertTo-SecureString -String $token -AsPlainText -Force)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $H -Path (Join-Path $credDir 'bot.json')), (ConvertTo-Json -Compress -InputObject ([ordered]@{ format = 'localswim-credential'; format_version = 1; role = 'bot'; token_dpapi_hex = $hex })))
  [pscustomobject]@{ Token = $token; Content = "LOCALSWIM_URL=$HpBaseUrl`nLOCALSWIM_TOKEN=$token`n" }
}
function Get-HpSha256Hex([string]$Text) {
  [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([Text.UTF8Encoding]::new($false).GetBytes($Text))).Replace('-', '')
}
# The stamp the spec gives: the SHA-256 of <instance ID> LF <zone> LF <content>, as uppercase hex.
function Get-HpExpectedStamp($Fixture, [string]$InstanceId, [string]$Zone) { Get-HpSha256Hex ($InstanceId + "`n" + $Zone + "`n" + $Fixture.Content) }
# A stand-in ssh that acts as the box does (operator.md, "What the box does"): it reads the push's
# stdin, answers the hash line and the zone line, and records each call's arguments as one JSON line.
$HpSshScript = @'
$dir = $PSScriptRoot
$zone = (Get-Content -LiteralPath (Join-Path $dir 'ssh-zone.txt') -Raw).Trim()
$m = New-Object IO.MemoryStream
[Console]::OpenStandardInput().CopyTo($m)
$bytes = $m.ToArray()
[IO.File]::AppendAllText((Join-Path $dir 'ssh-args.log'), ((ConvertTo-Json -InputObject @($args) -Compress) + "`n"))
$text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r", '')
if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
$lines = @($text -split "`n" | Where-Object { $_ -ne '' })
$norm = (($lines -join "`n") + "`n")
$sha = [Security.Cryptography.SHA256]::Create()
$hash = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
[Console]::Out.Write($hash + '  -' + "`n")
if ($zone -ne '') { [Console]::Out.Write('zone ' + $zone + "`n") }
exit 0
'@
function New-HpPushSsh($H, [string]$Zone) {
  $dir = Join-Path $H.Root 'standins'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $dir) | Out-Null
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $H -Path (Join-Path $dir 'ssh-standin.ps1')), $HpSshScript)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $H -Path (Join-Path $dir 'ssh-zone.txt')), $Zone)
  New-OperatorStandIn -ConformanceHome $H -Name 'ssh-push' -Body @(
    'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0ssh-standin.ps1" %*',
    'exit /b %ERRORLEVEL%')
}
# The arguments of the n-th push ssh, as the stand-in recorded them, or $null.
function Get-HpPushArgs($S, [int]$N) {
  $line = @(Get-HpLines (Join-Path $S.Dir 'ssh-args.log'))[$N - 1]
  if (-not $line) { return $null }
  , @(ConvertFrom-Json -InputObject $line)
}
function Get-HpPushCount($S) { @(Get-HpLines (Join-Path $S.Dir 'ssh-args.log')).Count }
# The stamp a stamp file holds, {"version":0,"stamp":"<hex>"} (state.md, "The push stamp"), or $null when
# there is no file, or the file's whole text when it is not that document, so no check passes on it.
function Get-HpStampValue([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
  $raw = [IO.File]::ReadAllText($Path)
  try { $doc = ConvertFrom-Json -InputObject $raw -ErrorAction Stop } catch { return $raw }
  if ($doc -isnot [pscustomobject]) { return $raw }
  $names = @($doc.PSObject.Properties.Name)
  if ($names.Count -ne 2 -or $names -cnotcontains 'version' -or $names -cnotcontains 'stamp') { return $raw }
  if ($doc.version -isnot [long] -or $doc.version -ne 0 -or $doc.stamp -isnot [string]) { return $raw }
  $doc.stamp
}

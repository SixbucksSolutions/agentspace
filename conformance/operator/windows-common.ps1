#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code of the operator's Windows cases, W20 to W32 (docs/conformance.md, "The operator's Windows cases";
# docs/operator.md, "Windows hosts", "The platform seam"): test-unmet-windows-hosts.ps1,
# test-unmet-windows-deploy-sync.ps1, test-unmet-windows-guard.ps1 and test-unmet-windows-proxy.ps1 in
# conformance/operator/ (E0.12-T04-S02, card #0262). It dot-sources host-commands-common.ps1 and so
# helpers.ps1 and the harness, and reuses its fixture, registry helpers and case runner. This file starts no
# program under test itself and loads no program's code. Every function here has the prefix Wn.
#
# What it adds:
#   - The names a host takes from its platform (docs/operator.md, "A host's platform and its names").
#   - Seeding: a registry record with a platform, a lease record of any platform, the project list.
#   - Stand-ins that see more than host-commands-common.ps1's: an `ssh` that also says whether the token
#     was in its environment or in a file it could read, an `scp` that also records the files of each
#     local folder it was given, and a `go` that records the Go variables it saw. They replace the scripts
#     the fixture wrote, beside the same .cmd files, so every log keeps its form.
#   - Checks of the encoded remote command (docs/operator.md, "The remote command on Windows") and of the
#     JSON a Run Command is sent as `--parameters` (docs/operator.md, "The platform seam").
#
# SAFETY. Every write and delete this file makes calls Assert-ConformancePath at the call (docs/conformance.md,
# "The program under test is untrusted"); the stand-ins write only beside themselves, under the home's
# standins folder. It records no environment's values: the ssh stand-in logs whether a value held the token,
# never the value. Every identifier is made up, and a name that would look like an image or an instance ID to
# the identifier scan is built from two pieces.
# A file that also loads host-guard-common.ps1 loads that first, and so has host-commands-common.ps1 already: the
# harness and its mask MUST NOT be dot-sourced a second time (docs/conformance.md, "The shared mask").
if (-not (Get-Command -Name Invoke-HostFile -ErrorAction SilentlyContinue)) { . (Join-Path $PSScriptRoot 'host-commands-common.ps1') }

# Made-up identifiers: an image in the form of Amazon's, and instance IDs for the three hosts of one project.
$script:WnImage = 'ami' + '-' + '0a1b2c3d4e5f60718'
$script:WnIdWin = 'i' + '-' + 'c5c5c5c5c5c5c5c5c'
$script:WnIdArm = 'i' + '-' + 'a7a7a7a7a7a7a7a7a'
$script:WnIdAmd = 'i' + '-' + 'b6b6b6b6b6b6b6b6b'
$script:WnRegion = 'us-west-2'
$script:WnRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# ---- Names (docs/operator.md, "A host's platform and its names") ----

# The platform's name part, its one `/` written `-`; never for linux/arm64.
function Get-WnNamePart([string]$Platform) { $Platform.Replace('/', '-') }
# A host's name: the project on linux/arm64, `<project>.<os>-<arch>` on every other platform.
function Get-WnHostName([string]$Project, [string]$Platform = 'linux/arm64') {
  if ($Platform -ceq 'linux/arm64') { $Project } else { $Project + '.' + (Get-WnNamePart $Platform) }
}
# The stack: `agentspace-host-<project>` on linux/arm64, `agentspace-<os>-<arch>-host-<project>` otherwise.
function Get-WnStack([string]$Project, [string]$Platform = 'linux/arm64') {
  if ($Platform -ceq 'linux/arm64') { "agentspace-host-$Project" } else { 'agentspace-' + (Get-WnNamePart $Platform) + "-host-$Project" }
}
function Get-WnAlias([string]$Project, [string]$Platform = 'linux/arm64') { 'agentspace-host.' + (Get-WnHostName $Project $Platform) }
function Get-WnLoginUser([string]$Platform) { if ($Platform -ceq 'windows/amd64') { 'agent' } else { 'ubuntu' } }
function Get-WnType([string]$Platform) { if ($Platform -ceq 'linux/arm64') { 'm9g.xlarge' } else { 'm8a.xlarge' } }

# ---- The registry ----

# One registry record as text, its members in the table's order: the project, the platform (never written for
# linux/arm64), the stack, the state, the instance ID, the instance type and the last attempt.
function New-WnRecord {
  param(
    [Parameter(Mandatory)][string]$Project,
    [string]$Platform = 'linux/arm64',
    [string]$State = 'ready',
    [string]$InstanceId = '',
    [string]$InstanceType = '',
    [string]$LastAttempt = '',
    [switch]$NoInstanceType
  )
  if (-not $InstanceType) { $InstanceType = Get-WnType $Platform }
  $t = '{"project":"' + $Project + '"'
  if ($Platform -cne 'linux/arm64') { $t += ',"platform":"' + $Platform + '"' }
  $t += ',"stack":"' + (Get-WnStack $Project $Platform) + '","state":"' + $State + '"'
  if ($InstanceId) { $t += ',"instance_id":"' + $InstanceId + '"' }
  if ($State -ceq 'ready' -and -not $NoInstanceType) { $t += ',"instance_type":"' + $InstanceType + '"' }
  if ($LastAttempt) { $t += ',"last_attempt_at":"' + $LastAttempt + '"' }
  $t + '}'
}
# The three records of one project, seeded in an order that is not by project then platform (docs/conformance.md,
# W20; pin P46): windows/amd64 first, then linux/arm64, then linux/amd64, after the box's.
function Set-WnThreeHosts {
  param([string]$Project = 'example', [string]$WinState = 'ready', [string]$WinId = $script:WnIdWin, [switch]$NoWinId)
  $win = if ($NoWinId) { New-WnRecord -Project $Project -Platform 'windows/amd64' -State $WinState } else { New-WnRecord -Project $Project -Platform 'windows/amd64' -State $WinState -InstanceId $WinId }
  $arm = New-WnRecord -Project $Project -State 'ready' -InstanceId $script:WnIdArm
  $amd = New-WnRecord -Project $Project -Platform 'linux/amd64' -State 'ready' -InstanceId $script:WnIdAmd
  Set-HostsRecords @((New-BoxRecord), $win, $arm, $amd)
  [pscustomobject]@{ Win = $win; Arm = $arm; Amd = $amd }
}
# Every answer of healthy hosts for the project's three instances and the box's.
function Get-WnHealthy {
  param([string]$Project = 'example', [string]$State = 'running', [string]$Launch = '2020-01-01T00:00:00+00:00')
  $s = @{}
  foreach ($h in @(@($script:WnIdWin, 'windows/amd64'), @($script:WnIdArm, 'linux/arm64'), @($script:WnIdAmd, 'linux/amd64'))) {
    $one = Get-HealthyAnswers -Id $h[0] -State $State -Type (Get-WnType $h[1])
    $one["ec2 describe-instances $($h[0])"] = @(New-Step (New-InstanceDoc -Id $h[0] -State $State -Type (Get-WnType $h[1]) -Launch $Launch))
    $s = Merge-Scenario $s $one
  }
  Merge-Scenario $s (Get-HealthyAnswers -Id $script:IdBox)
}
# The stack answers of the three hosts, each existing and holding its instance.
function Get-WnStacks([string]$Project = 'example') {
  @{
    "cloudformation describe-stacks $(Get-WnStack $Project 'windows/amd64')" = @(New-Step (New-StackDoc -StackName (Get-WnStack $Project 'windows/amd64') -Id $script:WnIdWin -InstanceType (Get-WnType 'windows/amd64')))
    "cloudformation describe-stacks $(Get-WnStack $Project 'linux/arm64')" = @(New-Step (New-StackDoc -StackName (Get-WnStack $Project 'linux/arm64') -Id $script:WnIdArm -InstanceType (Get-WnType 'linux/arm64')))
    "cloudformation describe-stacks $(Get-WnStack $Project 'linux/amd64')" = @(New-Step (New-StackDoc -StackName (Get-WnStack $Project 'linux/amd64') -Id $script:WnIdAmd -InstanceType (Get-WnType 'linux/amd64')))
  }
}

# ---- The project list and the lease state file ----

# Writes projects.json with the entries given as text, each `{"name":...}`.
function Set-WnProjects([string[]]$Entry) {
  $fx = $script:Fx
  $text = '{"version":0,"projects":[' + ($Entry -join ',') + ']}' + "`n"
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Agentspace 'projects.json')), [Text.UTF8Encoding]::new($false).GetBytes($text))
}
function New-WnProjectEntry([string]$Name, [string[]]$Platform = @()) {
  $t = '{"name":"' + $Name + '","approved":"2026-11-20"'
  if ($Platform.Count -gt 0) { $t += ',"platforms":[' + (($Platform | ForEach-Object { '{"platform":"' + $_ + '","approved":"2026-11-27"}' }) -join ',') + ']' }
  $t + '}'
}
$script:WnLeaseN = 0
# One lease record as text, for any platform. A ready one needs -Expires; a released one -Released (and -Expires too).
function New-WnLease {
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State,
    [string]$Platform = 'linux/arm64',
    [Nullable[DateTime]]$Expires = $null,
    [Nullable[DateTime]]$Released = $null
  )
  $script:WnLeaseN++
  $n = $script:WnLeaseN
  $profile1 = if ($Platform -ceq 'windows/amd64') { 'agent-windows' } else { 'agent-linux' }
  $r = [ordered]@{ id = ('{0:x8}-0000-4000-8000-{1:x12}' -f ($n + 4096), ($n + 4096)); project = $Project; state = $State; platform = $Platform; profile = $profile1; size = 'm'; ttl_seconds = 3600; created_at = '2026-01-01T00:00:00Z' }
  if ($null -ne $Expires) { $r.expires_at = Format-WnTime $Expires }
  if ($null -ne $Released) { $r.released_at = Format-WnTime $Released }
  if ($State -eq 'ready') { $r.ssh = [ordered]@{ host_alias = ('agentspace-{0}-{1:x8}' -f $Project, ($n + 4096)); user = (Get-WnLoginUser $Platform) } }
  if ($State -eq 'unsatisfiable') { $r.reasons = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
  $r.driver = 'ec2'
  ConvertTo-Json -Compress -Depth 6 -InputObject $r
}
function Format-WnTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
# The lease alias of the lease record whose text is given (`agentspace-<project>-<first 8 digits of its ID>`).
function Get-WnLeaseAlias([string]$Record) { $d = ConvertFrom-Json -InputObject $Record; "agentspace-$($d.project)-$($d.id.Substring(0, 8))" }
function Set-WnLeases([string[]]$Record = @()) {
  $fx = $script:Fx
  $text = '{"version":0,"leases":[' + ($Record -join ',') + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text -Schema 'state/leases.state.schema.json')) { throw 'The seeded lease state file does not validate against its schema.' }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.H.StateDir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.H.StateDir 'leases.json')), [Text.UTF8Encoding]::new($false).GetBytes($text))
}
function Remove-WnLeases { Remove-HomePath (Join-Path $script:Fx.H.StateDir 'leases.json') }
# A CLAUDE.md for sync's steps 5 and 6.
function Set-WnClaudeMd {
  $fx = $script:Fx
  $dir = Join-Path $fx.H.UserProfile '.claude'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $dir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $dir 'CLAUDE.md')), [Text.UTF8Encoding]::new($false).GetBytes("A made-up instruction file.`n"))
}

# ---- The stand-ins ----

# The ssh stand-in of host-commands-common.ps1, and what it adds: whether the token the check names in
# token-needle.txt beside it was in any argument, in any environment value, or in any file under the home's
# `.agentspace` folder. It records the answers, never a value.
$script:WnSshScript = @'
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
$needleFile = Join-Path $dir 'token-needle.txt'
$entry.tokenArg = $false; $entry.tokenEnv = $false; $entry.tokenFile = $false
if (Test-Path -LiteralPath $needleFile) {
  $needle = [IO.File]::ReadAllText($needleFile).Trim()
  if ($needle.Length -gt 0) {
    foreach ($x in $a) { if ($x.Contains($needle)) { $entry.tokenArg = $true } }
    foreach ($e in [Environment]::GetEnvironmentVariables().GetEnumerator()) { if (([string]$e.Value).Contains($needle)) { $entry.tokenEnv = $true } }
    foreach ($f in Get-ChildItem -LiteralPath (Join-Path $up '.agentspace') -Recurse -File -Force -ErrorAction SilentlyContinue) {
      if ($f.Length -lt 1048576) { try { if ([IO.File]::ReadAllText($f.FullName).Contains($needle)) { $entry.tokenFile = $true } } catch { } }
    }
  }
}
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

# The scp stand-in of host-commands-common.ps1, and what it adds: for each local folder, `map`, its files by
# relative path (forward slashes) to their SHA-256, so a case can compare what was sent with the repository's own.
$script:WnScpScript = @'
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
    $map = [ordered]@{}
    $dirs = @()
    if ($isDir) {
      $root = (Get-Item -LiteralPath $x).FullName.TrimEnd('\')
      $dirs = @(Get-ChildItem -LiteralPath $x -Recurse -Directory -Force | ForEach-Object { $_.FullName.Substring($root.Length + 1).Replace('\', '/') })
      foreach ($f in Get-ChildItem -LiteralPath $x -Recurse -File -Force) {
        $map[$f.FullName.Substring($root.Length + 1).Replace('\', '/')] = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($f.FullName))).Replace('-', '').ToLowerInvariant()
      }
    }
    $locals += [ordered]@{
      path = $x
      exists = [bool](Test-Path -LiteralPath $x)
      isDir = $isDir
      leaf = (Split-Path -Leaf $x)
      map = $map
      dirs = @($dirs)
      sha = $(if (-not $isDir -and (Test-Path -LiteralPath $x -PathType Leaf)) { [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($x))).Replace('-', '').ToLowerInvariant() } else { '' })
      parentLeaf = $(if ($p) { Split-Path -Leaf $p } else { '' })
      opLeaf = $(if ($op) { Split-Path -Leaf $op } else { '' })
      underOperator = [bool]($x.StartsWith((Join-Path $up '.agentspace\operator'), [StringComparison]::OrdinalIgnoreCase))
    }
  }
}
$entry.locals = @($locals)
[IO.File]::AppendAllText((Join-Path $dir 'scp-args.log'), ((ConvertTo-Json -Compress -Depth 8 -InputObject $entry) + "`n"))
$f = Join-Path $dir 'ans-scp.json'
if (Test-Path -LiteralPath $f) {
  $s = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f))
  if ($s.err) { [Console]::Error.Write([string]$s.err); [Console]::Error.Flush() }
  if ($s.out) { [Console]::Out.Write([string]$s.out); [Console]::Out.Flush() }
  exit ([int]$s.code)
}
exit 0
'@

# The go stand-in: logs its arguments, its working folder and the three Go variables it saw, and writes a small
# file at the `-o` path when that path is under the home. It is found through the PATH a run names.
$script:WnGoScript = @'
$dir = $PSScriptRoot
$a = @($args | ForEach-Object { [string]$_ })
$goenv = [ordered]@{}
foreach ($k in 'GOOS', 'GOARCH', 'CGO_ENABLED') { $v = [Environment]::GetEnvironmentVariable($k); if ($null -ne $v) { $goenv[$k] = $v } }
$entry = [ordered]@{ a = $a; cwd = (Get-Location).Path; goenv = $goenv }
[IO.File]::AppendAllText((Join-Path $dir 'go-args.log'), ((ConvertTo-Json -Compress -Depth 4 -InputObject $entry) + "`n"))
$i = [array]::IndexOf($a, '-o')
if ($i -ge 0 -and $i + 1 -lt $a.Count) {
  $out = [IO.Path]::GetFullPath($a[$i + 1])
  if ($out.StartsWith($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase)) { [IO.File]::WriteAllText($out, 'fake localswim build') }
}
exit 0
'@

# Replaces the fixture's ssh and scp scripts with the two above, beside the same .cmd files, and writes the go
# stand-in and its .cmd, and puts that folder first on the fixture's PATH. Returns the folder that holds go.cmd. Call
# after the fixture exists.
function Initialize-WnStandIns {
  $fx = $script:Fx
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Standins 'ssh-standin.ps1')), $script:WnSshScript)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Standins 'scp-standin.ps1')), $script:WnScpScript)
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Standins 'go-standin.ps1')), $script:WnGoScript)
  $goDir = Join-Path $fx.H.Root 'gobin'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $goDir) | Out-Null
  $pwsh = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
  Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $goDir 'go.cmd')) -Encoding ascii -Value @('@echo off', ('"' + $pwsh + '" -NoProfile -File "' + (Join-Path $fx.Standins 'go-standin.ps1') + '" %*'), 'exit /b %ERRORLEVEL%')
  # The fixture's own PATH is an empty folder and Windows' own; from here every run finds the go stand-in first, so no
  # run's -Environment names a PATH (host-commands-common.ps1 adds the two, and a key named twice is an error).
  $fx.Env['PATH'] = Get-WnGoPath $goDir
  $goDir
}
# The PATH of a run that finds the go stand-in first, and Windows' own folder.
function Get-WnGoPath([string]$GoDir) { $GoDir + ';' + (Join-Path $env:SystemRoot 'System32') }
function Get-WnGoCalls { Read-JsonLog (Join-Path $script:Fx.Standins 'go-args.log') }
# Tells the ssh stand-in which token to look for. The file is beside the stand-in, in the home.
function Set-WnTokenNeedle([string]$Token) {
  $fx = $script:Fx
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.Standins 'token-needle.txt')), $Token)
}
# The localswim checkout the sync's build needs (docs/operator.md, sync, step 7): one Test-Path on the folder.
function Test-WnLocalswimFolder { Test-Path -LiteralPath 'C:\Projects\localswim' -PathType Container }

# ---- Reading the calls ----

# The argument after a flag, or $null.
function Get-WnFlag($Call, [string]$Flag) {
  $i = [array]::IndexOf($Call.a, $Flag)
  if ($i -lt 0 -or $i + 1 -ge $Call.a.Count) { return $null }
  $Call.a[$i + 1]
}
# The remote command of an ssh call: its last argument.
function Get-WnRemote($Call) { if ($Call.a.Count -eq 0) { $null } else { [string]$Call.a[$Call.a.Count - 1] } }
# The ssm send-command calls of a run, and the command's JSON.
function Get-WnSends { @(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[0] -ceq 'ssm' -and $_.a[1] -ceq 'send-command' }) }
# The `--parameters` argument parsed as JSON, or $null when it is not JSON.
function Get-WnParams($Call) {
  $v = Get-WnFlag $Call '--parameters'
  if ($null -eq $v) { return $null }
  try { ConvertFrom-Json -InputObject $v -ErrorAction Stop } catch { $null }
}
# Whether a call names a value anywhere, in any argument.
function Test-WnNames($Call, [string]$Value) { foreach ($x in $Call.a) { if ($x.Contains($Value)) { return $true } }; $false }

# ---- The encoded remote command (docs/operator.md, "The remote command on Windows") ----

# The script an encoded command carries, or $null when the command is not of the form or does not decode.
function Get-WnScript([string]$Command) {
  $m = [regex]::Match([string]$Command, '^powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand (?<b>[A-Za-z0-9+/]+)$')
  if (-not $m.Success) { return $null }
  $b = $m.Groups['b'].Value
  if ($b.Length % 4 -ne 0) { return $null }
  try { [Text.UnicodeEncoding]::new($false, $false, $true).GetString([Convert]::FromBase64String($b)) } catch { $null }
}
# Problems of one remote command: of the encoded form, at most 8,191 characters, its base64 with no `=` and a
# multiple of 4 characters, and decoding to UTF-16LE.
function Get-WnEncodedProblems([string]$Command, [string]$What) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ([string]::IsNullOrEmpty($Command)) { $p.Add("$What has no remote command"); return $p.ToArray() }
  if ($Command.Length -gt 8191) { $p.Add("$What's remote command is $($Command.Length) characters, over 8,191") }
  if ($Command.Contains('=')) { $p.Add("$What's remote command holds an equals sign") }
  $m = [regex]::Match($Command, '^powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand (?<b>[A-Za-z0-9+/]+)$')
  if (-not $m.Success) { $p.Add("$What's remote command is not powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand <b64>"); return $p.ToArray() }
  if ($m.Groups['b'].Value.Length % 4 -ne 0) { $p.Add("$What's base64 is not a multiple of 4 characters, so the script was not padded with spaces to a multiple of 3 bytes") ; return $p.ToArray() }
  if ($null -eq (Get-WnScript $Command)) { $p.Add("$What's base64 does not decode to UTF-16LE") }
  $p.ToArray()
}
# The repository's box\windows files as a map from a file's name to its lines, trimmed, without the empty ones and
# the comment ones (those that start with #). It is read once and kept ("none holds the text of a box/windows
# file", docs/conformance.md, W32).
function Get-WnBoxWindowsFiles {
  if ($null -ne $script:WnBoxFiles) { return $script:WnBoxFiles }
  $root = Join-Path $script:WnRepo 'box\windows'
  $map = @{}
  foreach ($f in Get-ChildItem -LiteralPath $root -Recurse -File -Force) {
    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($l in [IO.File]::ReadAllLines($f.FullName)) { $t = $l.Trim(); if ($t.Length -gt 0 -and -not $t.StartsWith('#')) { $lines.Add($t) } }
    $map[$f.Name] = $lines.ToArray()
  }
  $script:WnBoxFiles = $map
  $map
}
# Problems: the remote command, or the script it carries, holds the text of a box\windows file. By default, four
# consecutive lines of any one file (a line alone may be a common statement, four in a row are a file's text); with
# -LineOf <file name>, any one line of 30 characters or more of that file alone.
function Get-WnLeakProblems {
  param([string]$Command, [string]$What, [string]$LineOf = '')
  $p = [System.Collections.Generic.List[string]]::new()
  $text = $Command + "`n" + (Get-WnScript $Command)
  $files = Get-WnBoxWindowsFiles
  if ($LineOf) {
    if (-not $files.ContainsKey($LineOf)) { $p.Add("The box\windows file $LineOf does not exist, so the check could not look for it"); return $p.ToArray() }
    foreach ($l in $files[$LineOf]) { if ($l.Length -ge 30 -and $text.Contains($l)) { $p.Add("$What's remote command holds a line of $LineOf"); break } }
    return $p.ToArray()
  }
  $norm = (@(($text -replace "`r", '') -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_.Length -gt 0 -and -not $_.StartsWith('#') }) -join "`n")
  foreach ($name in $files.Keys) {
    $ls = $files[$name]
    for ($i = 0; $i + 3 -lt $ls.Count; $i++) {
      if ($norm.Contains(($ls[$i..($i + 3)] -join "`n"))) { $p.Add("$What's remote command holds four consecutive lines of the box\windows file $name"); break }
    }
  }
  $p.ToArray()
}
# The problems that bind every remote command a stand-in ssh saw for a Windows host (docs/conformance.md, W32):
# the encoded form, no line of a box\windows file, and the token in no argument, environment value or file.
function Get-WnSshProblems($Calls) {
  $p = [System.Collections.Generic.List[string]]::new()
  $i = 0
  foreach ($c in @($Calls)) {
    $i++
    $cmd = Get-WnRemote $c
    foreach ($m in (Get-WnEncodedProblems $cmd "ssh call $i")) { $p.Add($m) }
    foreach ($m in (Get-WnLeakProblems $cmd "ssh call $i")) { $p.Add($m) }
    if ($c.tokenArg -or $c.tokenEnv -or $c.tokenFile) { $p.Add("ssh call ${i}: the token was in an argument, an environment value or a file") }
  }
  $p.ToArray()
}
# Problems: an ssm send-command of a run is not for a Windows host's PowerShell document with its parameters as one
# JSON argument, `{"commands":["<command>"]}`, an array of one string, never the shorthand `commands=`.
function Get-WnSendProblems($Call, [string]$What, [string]$Comment = '') {
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-WnFlag $Call '--document-name') -cne 'AWS-RunPowerShellScript') { $p.Add("$What does not name AWS-RunPowerShellScript") }
  if ($Comment -and (Get-WnFlag $Call '--comment') -cne $Comment) { $p.Add("$What's comment is not '$Comment'") }
  $v = Get-WnFlag $Call '--parameters'
  if ($null -eq $v) { $p.Add("$What has no --parameters"); return $p.ToArray() }
  if ($v.StartsWith('commands=')) { $p.Add("$What passes the shorthand commands=") }
  $j = Get-WnParams $Call
  if ($null -eq $j) { $p.Add("$What's --parameters does not parse as JSON") }
  else {
    $names = @($j.PSObject.Properties.Name)
    $cmds = @($j.commands)
    if ($names.Count -ne 1 -or $names[0] -cne 'commands') { $p.Add("$What's JSON has members other than commands alone") }
    elseif ($cmds.Count -ne 1 -or $cmds[0] -isnot [string]) { $p.Add("$What's commands array does not hold exactly one string") }
  }
  $p.ToArray()
}
# The one command string of a send's JSON, or $null.
function Get-WnSendCommand($Call) {
  $j = Get-WnParams $Call
  if ($null -eq $j -or @($j.commands).Count -ne 1) { return $null }
  [string]@($j.commands)[0]
}

# ---- What a run left ----

# The host folders under hosts\ that exist.
function Get-WnHostFolders {
  $fx = $script:Fx
  if (-not (Test-Path -LiteralPath $fx.HostsDir)) { return @() }
  @(Get-ChildItem -LiteralPath $fx.HostsDir -Directory -Force | ForEach-Object { $_.Name })
}
# The host registry's text, or $null.
function Get-WnHostsText { $fx = $script:Fx; if (Test-Path -LiteralPath $fx.HostsFile -PathType Leaf) { [IO.File]::ReadAllText($fx.HostsFile) } else { $null } }
# Problems: a record's text is still in the registry, and the order of three records is the order given.
function Get-WnOtherRecordProblems([string]$Text, [string[]]$Keep) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ($null -eq $Text) { $p.Add('hosts.json is missing'); return $p.ToArray() }
  $last = -1
  foreach ($r in $Keep) {
    $i = $Text.IndexOf($r, [StringComparison]::Ordinal)
    if ($i -lt 0) { $p.Add('A record that the command does not act on was changed or removed') }
    elseif ($i -lt $last) { $p.Add('The records were moved: they are not in the order they were seeded in') }
    else { $last = $i }
  }
  $p.ToArray()
}
# Problems: a call names a host that is not the one acted on (its stack, its instance ID or its alias).
function Get-WnForeignProblems($Calls, [string[]]$Foreign, [string]$What) {
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($c in @($Calls)) {
    foreach ($f in $Foreign) { if (Test-WnNames $c $f) { $p.Add("$What carries a value of another host's"); break } }
  }
  $p.ToArray()
}
# The reason a failure quotes, with every path masked.
function Get-WnMasked([string]$Text) { Format-QuotedSpec $Text }

# ---- Time ----
function Get-WnNow { $t = [DateTime]::UtcNow; [DateTime]::new($t.Ticks - ($t.Ticks % 10000000), [DateTimeKind]::Utc) }
function Format-WnAwsTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'+00:00'", [Globalization.CultureInfo]::InvariantCulture) }
function Get-WnUnix([DateTime]$T) { [DateTimeOffset]::new($T.ToUniversalTime()).ToUnixTimeSeconds() }

# ---- deploy's calls (docs/operator.md, "Calls by command") ----
$script:WnTemplatePattern = 're:\\\.agentspace\\operator\\[0-9a-f]{32}\\agentspace\.yaml$'
function Get-WnDoesNotExist([string]$Stack) { "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $Stack does not exist" }
# The `cloudformation deploy` call as a list of arguments: the stack, the template in the command's own folder, the
# capability, the empty-change-set flag, the tag, then the overrides when there are any.
function Get-WnDeployCall([string]$Stack, [string[]]$Overrides = @()) {
  $a = @('cloudformation', 'deploy', '--region', $script:WnRegion, '--stack-name', $Stack, '--template-file', $script:WnTemplatePattern,
    '--capabilities', 'CAPABILITY_IAM', '--no-fail-on-empty-changeset', '--tags', "project=$Stack")
  if ($Overrides.Count -gt 0) { $a += '--parameter-overrides'; $a += $Overrides }
  , $a
}
function Get-WnCallGetParameter([string]$Name) { "ssm get-parameter --region $($script:WnRegion) --output json --name $Name" }
$script:WnImageParameter = '/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base'
function Get-WnImageAnswer([string]$Value) { New-Step ('{"Parameter":{"Value":"' + $Value + '"}}') }

# ---- The repository's own files, for what a copy MUST carry ----

# A folder of the repository as relative path (forward slashes) to SHA-256, as the scp stand-in records a local folder.
function Get-WnRepoMap([string]$Relative) {
  $root = Join-Path $script:WnRepo $Relative
  $map = @{}
  foreach ($f in Get-ChildItem -LiteralPath $root -Recurse -File -Force) {
    $map[$f.FullName.Substring($root.TrimEnd('\').Length + 1).Replace('\', '/')] = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($f.FullName))).Replace('-', '').ToLowerInvariant()
  }
  $map
}
function Get-WnRepoFileSha([string]$Relative) {
  [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes((Join-Path $script:WnRepo $Relative)))).Replace('-', '').ToLowerInvariant()
}
# Problems: the folder an scp call recorded holds exactly the repository's files of -Relative. With -Without <name>
# (a top-level folder such as `tests`), the repository's files under it are left out of what the copy MUST hold, and
# the copy MUST hold no entry of that name at all, file or folder. operator.md's "sync on a Windows host" says: "`box/windows/tests\` is
# never carried, written or copied: the operator holds `box/windows`' files but its `tests\` folder, which runs only
# on the laptop (...), so the staging folder never holds a `tests` entry (architect, 2026-11-27, for v0.12-T04, pin P15)."
function Get-WnFolderProblems($Local, [string]$Relative, [string]$What, [string]$Without = '') {
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not $Local.isDir) { $p.Add("$What was not a folder at the time of the copy"); return $p.ToArray() }
  $want = Get-WnRepoMap $Relative
  $got = $Local.map
  $wantKeys = @($want.Keys | Where-Object { -not $Without -or -not $_.StartsWith($Without + '/', [StringComparison]::OrdinalIgnoreCase) })
  if ($Without) {
    $entries = @(@($got.Keys) + @($Local.dirs) | Where-Object { $_ -ieq $Without -or $_.StartsWith($Without + '/', [StringComparison]::OrdinalIgnoreCase) })
    if ($entries.Count -gt 0) { $p.Add("$What holds a $Without entry, which sync never carries") }
  }
  if ((($got.Keys | Sort-Object) -join ',') -cne (($wantKeys | Sort-Object) -join ',')) { $p.Add("$What does not hold exactly the files of the repository's $Relative$(if ($Without) { " without its $Without folder" })") }
  else { foreach ($k in $got.Keys) { if (-not $want.ContainsKey($k) -or $got[$k] -cne $want[$k]) { $p.Add("$What's $k is not the repository's file"); break } } }
  $p.ToArray()
}
# The ssh and scp calls of a run as one list in the order they started, each {K, T, C}.
function Get-WnTimeline {
  $l = [System.Collections.Generic.List[object]]::new()
  foreach ($c in @(Get-SshCalls)) { $l.Add([pscustomobject]@{ K = 'ssh'; T = [long]$c.t; C = $c }) }
  foreach ($c in @(Get-ScpCalls)) { $l.Add([pscustomobject]@{ K = 'scp'; T = [long]$c.t; C = $c }) }
  @($l | Sort-Object T)
}

# ---- Which alias the calls name ----

# Problems: every ssh call's alias (the argument before its remote command) and every scp call's `<alias>:` target is
# -Want, and none is a -Foreign alias. -AnyRequired adds one when no ssh or scp call was made at all.
function Get-WnAliasProblems {
  param($Ssh, $Scp, [Parameter(Mandatory)][string]$Want, [string[]]$Foreign = @(), [switch]$AnyRequired)
  $p = [System.Collections.Generic.List[string]]::new()
  $seen = 0
  foreach ($c in @($Ssh)) {
    $seen++
    $al = if ($c.a.Count -ge 2) { $c.a[$c.a.Count - 2] } else { '' }
    if ($al -cne $Want) { $p.Add("An ssh call names another alias than the host's") }
  }
  foreach ($c in @($Scp)) {
    $seen++
    $targets = @($c.a | Where-Object { $_ -cmatch '^agentspace[^:\\/ ]*:' })
    if ($targets.Count -eq 0) { $p.Add('An scp call names no alias') }
    foreach ($t in $targets) { if (-not $t.StartsWith($Want + ':', [StringComparison]::Ordinal)) { $p.Add("An scp call names another alias than the host's") } }
  }
  foreach ($c in (@($Ssh) + @($Scp))) {
    foreach ($f in $Foreign) { foreach ($x in $c.a) { if ($x -ceq $f -or $x.StartsWith($f + ':', [StringComparison]::Ordinal)) { $p.Add("A call names another platform's alias") } } }
  }
  if ($AnyRequired -and $seen -eq 0) { $p.Add('No ssh or scp call was made') }
  @($p | Select-Object -Unique)
}

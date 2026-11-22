#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code for the operator's per-host conformance files that drive `provision`, `bootstrap` and
# `sync` through the test seam (docs/operator.md, "The test seam"): test-unmet-host-provision*.ps1 and
# test-unmet-host-bootstrap-sync*.ps1. It dot-sources helpers.ps1 (and so the harness), starts no
# program of its own but the stand-ins it writes, and never starts the operator except through
# Invoke-ConformanceProgram.
#
# What it gives:
#   - One stand-in program for the AWS CLI, `ssh`, `scp` and `go` (a PowerShell 7 script that a .cmd
#     starts). It logs every call as one JSON line (arguments, the time, the class of an ssh, what
#     each local path argument held at the moment of the call, the operator's folders, whether
#     provision.lock was held), snapshots hosts.json at each call, records an ssh's standard input, and
#     answers from scenario.json: the AWS CLI by `<service> <operation>`, one answer per call with the
#     last one repeating, and ssh, scp and go by rules. A rule MAY make the stand-in stop at a named
#     hook until the check releases it, so that a check can change the throwaway home at a given point
#     of a run.
#     The three `ssm` calls of a finished patch check have default answers, `Online`, a command ID of
#     zeros and `patched` (docs/operator.md, The patch wait), which a scenario's own key replaces.
#   - A box that answers the token push as docs/operator.md, "What the box does", says.
#   - Seeding of the throwaway home (the host registry, the project list, the stack parameters, the
#     public key, a fake localswim receipt and credential), reading of the registry and the log, and
#     the runner that starts the operator with the seam set and `go` kept off PATH.
#
# SAFETY. Every stand-in writes beside itself, in <home root>\standins, and nowhere else but the
# operator's own `-o` output of the `go` stand-in, which it refuses unless it is under the home's root.
# Every write and delete this file makes calls Assert-ConformancePath at the call (docs/conformance.md,
# "The program under test is untrusted"). A stand-in records the names of environment variables and
# the values of the three named Go variables alone, never the values of an environment.

. (Join-Path $PSScriptRoot 'helpers.ps1')

$script:HpHelpers = Join-Path $PSScriptRoot 'helpers.ps1'
$script:HpPwsh = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$script:HpRepo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# The made-up values every file uses. None matches i-0[0-9a-f]{8,16}.
$script:HpBoxId = 'i-1234567890abcdef0'
$script:HpHostId = 'i-fedcba9876543210f'
$script:HpNewId = 'i-abcdef1234567890a'
$script:HpIdA = 'i-1111aaaa2222bbbb3'
$script:HpIdB = 'i-2222aaaa3333bbbb4'
$script:HpKeyLine = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example'
$script:HpSignIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."

# ---- The stand-in program -------------------------------------------------------------------------
$script:HpStandInScript = @'
$dir = $PSScriptRoot
$kind = [string]$args[0]
$a = @($args | Select-Object -Skip 1 | ForEach-Object { [string]$_ })
$cfg = Get-Content -LiteralPath (Join-Path $dir 'scenario.json') -Raw | ConvertFrom-Json -AsHashtable
$seqFile = Join-Path $dir 'seq.txt'
$n = 1
if (Test-Path -LiteralPath $seqFile) { $n = [int](Get-Content -LiteralPath $seqFile -Raw).Trim() + 1 }
[IO.File]::WriteAllText($seqFile, [string]$n)
$userHome = [string]$cfg['home']

# The alias an ssh or scp names, and the class of an ssh.
$alias = ''
if ($kind -eq 'ssh' -and $a.Count -ge 2) { $alias = $a[$a.Count - 2] }
if ($kind -eq 'scp') {
  foreach ($x in $a) { if ($x -match '^(?<al>[A-Za-z0-9._-]{2,}):(?<p>.*)$') { $alias = $Matches['al']; break } }
}
$stdin = [byte[]]@()
if ($kind -eq 'ssh') {
  $m = New-Object IO.MemoryStream
  [Console]::OpenStandardInput().CopyTo($m)
  $stdin = $m.ToArray()
  New-Item -ItemType Directory -Force -Path (Join-Path $dir 'stdin') | Out-Null
  [IO.File]::WriteAllBytes((Join-Path $dir "stdin\$n.bin"), $stdin)
}
$class = ''
if ($kind -eq 'ssh') {
  if ($a -contains 'BatchMode=yes') { $class = 'push' }
  elseif ($stdin.Length -gt 0) { $class = 'bootstrap' }
  else { $class = 'sync' }
}

# What each local path argument holds right now.
$paths = @()
foreach ($x in $a) {
  if ($x -match '^[A-Za-z]:[\\/]') {
    $info = @{ arg = $x; type = 'none'; files = @{} }
    if (Test-Path -LiteralPath $x -PathType Container) {
      $info.type = 'dir'
      $root = (Get-Item -LiteralPath $x).FullName.TrimEnd('\')
      foreach ($f in Get-ChildItem -LiteralPath $x -Recurse -File -Force) {
        $info.files[$f.FullName.Substring($root.Length + 1).Replace('\', '/')] = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($f.FullName))).Replace('-', '').ToLowerInvariant()
      }
    } elseif (Test-Path -LiteralPath $x -PathType Leaf) {
      $info.type = 'file'
      $info.sha = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($x))).Replace('-', '').ToLowerInvariant()
    }
    $paths += $info
  }
}
# The operator's folders at this moment.
$opDirs = @{}
$opRoot = Join-Path $userHome '.agentspace\operator'
if (Test-Path -LiteralPath $opRoot -PathType Container) {
  foreach ($d in Get-ChildItem -LiteralPath $opRoot -Directory -Force) { $opDirs[$d.Name] = @(Get-ChildItem -LiteralPath $d.FullName -Force | ForEach-Object { $_.Name }) }
}
# Whether provision.lock is held right now: a read open with no sharing fails while a program holds it.
$lockHeld = $null
if ($cfg.ContainsKey('provisionLock') -and $cfg['provisionLock'] -and (Test-Path -LiteralPath $cfg['provisionLock'] -PathType Leaf)) {
  try { $fs = [IO.File]::Open($cfg['provisionLock'], 'Open', 'Read', 'None'); $fs.Dispose(); $lockHeld = $false } catch { $lockHeld = $true }
}
$envNames = @()
$goEnv = @{}
if ($kind -eq 'go') {
  foreach ($k in 'GOOS', 'GOARCH', 'CGO_ENABLED') { $v = [Environment]::GetEnvironmentVariable($k); if ($null -ne $v) { $goEnv[$k] = $v } }
}
# Snapshot of the host registry at this call.
New-Item -ItemType Directory -Force -Path (Join-Path $dir 'snap') | Out-Null
$hostsFile = Join-Path $userHome '.agentspace\hosts.json'
try { if (Test-Path -LiteralPath $hostsFile -PathType Leaf) { [IO.File]::WriteAllBytes((Join-Path $dir "snap\$n.json"), [IO.File]::ReadAllBytes($hostsFile)) } } catch { }

$prev = @()
$callsFile = Join-Path $dir 'calls.jsonl'
if (Test-Path -LiteralPath $callsFile) { $prev = @(Get-Content -LiteralPath $callsFile | Where-Object { $_.Trim() -ne '' } | ForEach-Object { ConvertFrom-Json -InputObject $_ -AsHashtable }) }
$entry = [ordered]@{ n = $n; kind = $kind; t = [string][DateTime]::UtcNow.Ticks; a = $a; alias = $alias; class = $class; paths = $paths; opdirs = $opDirs; lockHeld = $lockHeld; cwd = (Get-Location).Path; goenv = $goEnv }
[IO.File]::AppendAllText($callsFile, ((ConvertTo-Json -InputObject $entry -Compress -Depth 8) + "`n"))

# Which answer: aws by <service> <operation>; the others by rules.
$step = $null
if ($kind -eq 'aws') {
  $key = ($a | Select-Object -First 2) -join ' '
  $same = @($prev | Where-Object { $_['kind'] -eq 'aws' -and (($_['a'] | Select-Object -First 2) -join ' ') -eq $key }).Count
  $steps = $null
  if ($cfg.ContainsKey('aws') -and $cfg['aws'].ContainsKey($key)) { $steps = @($cfg['aws'][$key]) }
  if ($steps -and $steps.Count -gt 0) { $step = $steps[[Math]::Min($same, $steps.Count - 1)] }
  # The default answers of the three Session Manager calls of a patch check that has finished (docs/operator.md,
  # Calls by command; The patch wait). A case that gives a key of its own for one of them keeps its answer.
  if ($null -eq $step) {
    $default = switch ($key) {
      'ssm describe-instance-information' { '{"InstanceInformationList":[{"PingStatus":"Online"}]}' }
      'ssm send-command' { '{"Command":{"CommandId":"00000000-0000-0000-0000-000000000000"}}' }
      'ssm get-command-invocation' { '{"Status":"Success","StandardOutputContent":"patched\n"}' }
      default { $null }
    }
    if ($null -ne $default) { $step = @{ out = $default; code = 0 } }
  }
} elseif ($kind -eq 'go') {
  $step = if ($cfg.ContainsKey('go')) { $cfg['go'] } else { @{} }
} else {
  $rules = @(); if ($cfg.ContainsKey('rules')) { $rules = @($cfg['rules']) }
  $sameKind = @($prev | Where-Object { $_['kind'] -eq $kind }).Count
  $sameAlias = @($prev | Where-Object { $_['kind'] -eq $kind -and $_['alias'] -eq $alias }).Count
  $sameClass = @($prev | Where-Object { $_['kind'] -eq $kind -and $_['class'] -eq $class }).Count
  foreach ($r in $rules) {
    if ($r['kind'] -ne $kind) { continue }
    if ($r.ContainsKey('class') -and $r['class'] -ne $class) { continue }
    if ($r.ContainsKey('alias') -and $r['alias'] -ne $alias) { continue }
    if ($r.ContainsKey('nth')) {
      $count = if ($r.ContainsKey('alias')) { $sameAlias } elseif ($r.ContainsKey('class')) { $sameClass } else { $sameKind }
      if ($r['nth'] -ne ($count + 1)) { continue }
    }
    $step = $r; break
  }
}
if ($null -ne $step -and $step.ContainsKey('hook') -and $step['hook']) {
  [IO.File]::WriteAllText((Join-Path $dir ('hook-' + $step['hook'] + '.reached')), 'x')
  $until = [DateTime]::UtcNow.AddSeconds(120)
  while (-not (Test-Path -LiteralPath (Join-Path $dir ('hook-' + $step['hook'] + '.go'))) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
}
$code = 0
if ($null -ne $step) {
  if ($step.ContainsKey('out') -and $step['out']) { [Console]::Out.Write([string]$step['out']); [Console]::Out.Flush() }
  if ($step.ContainsKey('err') -and $step['err']) { [Console]::Error.Write([string]$step['err']); [Console]::Error.Flush() }
  if ($step.ContainsKey('code')) { $code = [int]$step['code'] }
}
if ($kind -eq 'go' -and $code -eq 0) {
  $i = [Array]::IndexOf($a, '-o')
  if ($i -ge 0 -and $i + 1 -lt $a.Count) {
    $outPath = [IO.Path]::GetFullPath($a[$i + 1])
    $rootFull = [IO.Path]::GetFullPath([string]$cfg['root']).TrimEnd('\') + '\'
    if ($outPath.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) { [IO.File]::WriteAllText($outPath, 'fake localswim build') }
  }
}
# The box's half of the token push (docs/operator.md, "What the box does"), unless a rule answered it.
if ($kind -eq 'ssh' -and $class -eq 'push' -and ($null -eq $step) -and $code -eq 0) {
  $text = [Text.Encoding]::UTF8.GetString($stdin).Replace("`r", '')
  if ($text.Length -gt 0 -and $text[0] -eq [char]0xFEFF) { $text = $text.Substring(1) }
  $lines = @($text -split "`n" | Where-Object { $_ -ne '' })
  $norm = (($lines -join "`n") + "`n")
  $hash = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($norm))).Replace('-', '').ToLowerInvariant()
  [Console]::Out.Write($hash + '  -' + "`n")
  if ($cfg.ContainsKey('zone') -and $cfg['zone']) { [Console]::Out.Write('zone ' + $cfg['zone'] + "`n") }
}
exit $code
'@

# ---- Writing files, always through the assert -----------------------------------------------------
function Write-HpFile {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyString()][string]$Text)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $H -Path $Path), [Text.UTF8Encoding]::new($false).GetBytes($Text))
}
function Write-HpBytes {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)][string]$Path, [AllowNull()]$Bytes)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path (Split-Path -Parent $Path)) | Out-Null
  if ($null -eq $Bytes) { $Bytes = [byte[]]@() }
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $H -Path $Path), [byte[]]$Bytes)
}
function Remove-HpPath {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)][string]$Path)
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $H -Path $Path) -Recurse -Force }
}

# ---- Where things are ------------------------------------------------------------------------------
function Get-HpAgentspaceDir($H) { Join-Path $H.UserProfile '.agentspace' }
function Get-HpHostsFile($H) { Join-Path $H.UserProfile '.agentspace\hosts.json' }
function Get-HpHostFolder($H, [string]$Project) { Join-Path $H.UserProfile ".agentspace\hosts\$Project" }
function Get-HpLogFile($H, [string]$Project) { Join-Path $H.UserProfile ".agentspace\hosts\$Project\provision.log" }
function Get-HpLockFile($H, [string]$Project) { Join-Path $H.UserProfile ".agentspace\hosts\$Project\provision.lock" }
function Get-HpLeasesFile($H) { Join-Path $H.StateDir 'leases.json' }
function Get-HpStandInDir($H) { Join-Path $H.Root 'standins' }

# ---- The stand-ins ---------------------------------------------------------------------------------
# Writes the stand-in script and the three .cmd files (and a go.cmd in its own folder) into the home,
# and returns their paths. $Standins is the folder the scenario, the logs and the snapshots live in.
function New-HpStandIns {
  param([Parameter(Mandatory)]$H)
  $dir = Get-HpStandInDir $H
  Write-HpFile -H $H -Path (Join-Path $dir 'standin.ps1') -Text $script:HpStandInScript
  $r = [ordered]@{ Dir = $dir }
  foreach ($k in 'aws', 'ssh', 'scp') {
    $r[$k] = New-OperatorStandIn -ConformanceHome $H -Name $k -Body @(('"' + $script:HpPwsh + '" -NoProfile -File "%~dp0standin.ps1" ' + $k + ' %*'), 'exit /b %ERRORLEVEL%')
  }
  $goDir = Join-Path $H.Root 'gobin'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $goDir) | Out-Null
  $goCmd = Join-Path $goDir 'go.cmd'
  Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $H -Path $goCmd) -Encoding ascii -Value @('@echo off', ('"' + $script:HpPwsh + '" -NoProfile -File "' + (Join-Path $dir 'standin.ps1') + '" go %*'), 'exit /b %ERRORLEVEL%')
  $r['GoDir'] = $goDir
  [pscustomobject]$r
}

# Writes the scenario and forgets the calls, the snapshots, the hook flags and the counters of the last
# run. $Scenario holds aws (a hashtable of '<service> <operation>' to a list of steps), rules (ssh and
# scp), go (one step) and zone. A step is @{ out; err; code; hook }.
function Set-HpScenario {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [hashtable]$Scenario = @{}, [string]$LockProject = '')
  foreach ($f in @(Get-ChildItem -LiteralPath $S.Dir -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -in 'calls.jsonl', 'seq.txt' -or $_.Name -like '*-calls.log' -or $_.Name -like 'hook-*' -or $_.Name -in 'snap', 'stdin' })) {
    Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $H -Path $f.FullName) -Recurse -Force -ErrorAction SilentlyContinue
  }
  $doc = @{ home = $H.UserProfile; root = $H.Root; zone = 'America/New_York' }
  if ($LockProject) { $doc['provisionLock'] = Get-HpLockFile $H $LockProject }
  foreach ($k in $Scenario.Keys) { $doc[$k] = $Scenario[$k] }
  Write-HpFile -H $H -Path (Join-Path $S.Dir 'scenario.json') -Text (ConvertTo-Json -InputObject $doc -Depth 12 -Compress)
}

# ---- Answers of the AWS CLI -------------------------------------------------------------------------
function New-HpStep([string]$Out = '', [string]$Err = '', [int]$Code = 0, [string]$Hook = '') {
  $s = @{ out = $Out; err = $Err; code = $Code }
  if ($Hook) { $s['hook'] = $Hook }
  $s
}
function New-HpDescribe([string]$State = 'running', [string]$Launch = '2020-01-01T00:00:00+00:00') {
  New-HpStep (ConvertTo-Json -Compress -Depth 8 -InputObject @{ Reservations = @(@{ Instances = @(@{ State = @{ Name = $State }; LaunchTime = $Launch }) }) })
}
# A stack answer: its status and its InstanceId output (none when $Id is empty).
function New-HpStack([string]$Stack, [string]$Status = 'CREATE_COMPLETE', [string]$Id = '') {
  $st = @{ StackName = $Stack; StackStatus = $Status }
  if ($Id) { $st['Outputs'] = @(@{ OutputKey = 'InstanceId'; OutputValue = $Id }) } else { $st['Outputs'] = @() }
  New-HpStep (ConvertTo-Json -Compress -Depth 8 -InputObject @{ Stacks = @($st) })
}
function New-HpNoStack([string]$Stack) {
  New-HpStep '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $Stack does not exist" 254
}

# ---- Seeding the home ------------------------------------------------------------------------------
# The host registry as a writer writes it (docs/state.md, The host registry): the box's record first,
# each record's members in the table's order, compact, UTF-8 without a byte order mark, one LF.
function Get-HpHostsText {
  param([string]$BoxId = '', [string[]]$Records = @(), [string]$BoxLastAttempt = '')
  $box = '{"project":"swimcoach","stack":"agentbox","state":"ready"' + $(if ($BoxId) { ',"instance_id":"' + $BoxId + '"' } else { '' }) + $(if ($BoxLastAttempt) { ',"last_attempt_at":"' + $BoxLastAttempt + '"' } else { '' }) + '}'
  '{"version":0,"hosts":[' + ((@($box) + $Records) -join ',') + ']}' + "`n"
}
function New-HpRecord([string]$Project, [string]$State, [string]$Id = '', [string]$LastAttempt = '') {
  '{"project":"' + $Project + '","stack":"agentspace-host-' + $Project + '","state":"' + $State + '"' + $(if ($Id) { ',"instance_id":"' + $Id + '"' } else { '' }) + $(if ($LastAttempt) { ',"last_attempt_at":"' + $LastAttempt + '"' } else { '' }) + '}'
}
function Set-HpHosts($H, [string]$Text) { Write-HpFile -H $H -Path (Get-HpHostsFile $H) -Text $Text }
# The registry as parsed JSON: the records in order, or $null when there is no valid file.
function Read-HpHosts($H) {
  $f = Get-HpHostsFile $H
  if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { return $null }
  try { (ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f)) -AsHashtable -DateKind String).hosts } catch { $null }
}
function Get-HpRecord($H, [string]$Project) {
  $hosts = Read-HpHosts $H
  if ($null -eq $hosts) { return $null }
  foreach ($r in $hosts) { if ($r['project'] -ceq $Project) { return $r } }
  $null
}
function Get-HpRecordFromText([string]$Text, [string]$Project) {
  try { foreach ($r in (ConvertFrom-Json -InputObject $Text -AsHashtable -DateKind String).hosts) { if ($r['project'] -ceq $Project) { return $r } } } catch { }
  $null
}
function Get-HpFileBytes([string]$Path) { if (Test-Path -LiteralPath $Path -PathType Leaf) { [IO.File]::ReadAllBytes($Path) } else { $null } }
function Test-HpSameBytes($A, $B) {
  $x = if ($null -eq $A) { '' } else { [Convert]::ToBase64String([byte[]]$A) }
  $y = if ($null -eq $B) { '' } else { [Convert]::ToBase64String([byte[]]$B) }
  $x -ceq $y
}

# The files `deploy` and the project list need: the public key, the stack parameters (the box's own
# sizing members among them, which a host MUST NOT take), the project list and the registry.
function Initialize-HpHome {
  param(
    [Parameter(Mandatory)]$H,
    [string]$HostsText = (Get-HpHostsText -BoxId $script:HpBoxId),
    [string]$ProjectsText = '{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}' + "`n",
    [switch]$NoProjects
  )
  Write-HpFile -H $H -Path (Join-Path $H.UserProfile '.ssh\id_ed25519.pub') -Text "$($script:HpKeyLine)`r`n"
  Write-HpFile -H $H -Path (Join-Path $H.UserProfile '.agentspace\stack-parameters.json') -Text '{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE","InstanceType":"m9g.4xlarge","RootVolumeGiB":"64"}}'
  if ($HostsText) { Set-HpHosts $H $HostsText }
  if (-not $NoProjects) { Write-HpFile -H $H -Path (Join-Path $H.UserProfile '.agentspace\projects.json') -Text $ProjectsText }
}

# A fake localswim receipt and Bot credential (DPAPI-protected for the current user, as localswim's is),
# so that `sync`'s push is due and reaches the stand-in ssh. The token is made up and never printed.
function New-HpFakeLocalswim {
  param([Parameter(Mandatory)]$H)
  $base = 'http://127.0.0.1:18811'
  $token = 'conformance-fake-token-' + [guid]::NewGuid().ToString('N').Substring(0, 20)
  $lsDir = Join-Path $H.UserProfile 'AppData\Local\localswim-v2'
  $credDir = Join-Path $lsDir 'cred'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path $credDir) | Out-Null
  Write-HpFile -H $H -Path (Join-Path $lsDir 'service.json') -Text (ConvertTo-Json -Compress -InputObject @{ base = $base; credential_directory = $credDir })
  $hex = ConvertFrom-SecureString -SecureString (ConvertTo-SecureString -String $token -AsPlainText -Force)
  Write-HpFile -H $H -Path (Join-Path $credDir 'bot.json') -Text (ConvertTo-Json -Compress -InputObject ([ordered]@{ format = 'localswim-credential'; format_version = 1; role = 'bot'; token_dpapi_hex = $hex }))
  $token
}
# A CLAUDE.md for `sync`'s steps 5 and 6.
function New-HpClaudeMd($H) { Write-HpFile -H $H -Path (Join-Path $H.UserProfile '.claude\CLAUDE.md') -Text "A made-up instruction file.`n" }

# One lease record of the lease state file, as create writes a pending one.
function New-HpLeaseRecord {
  param([string]$Id, [string]$Project = 'agentspace', [string]$State = 'pending')
  $r = [ordered]@{ id = $Id; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 3600; created_at = '2026-11-21T00:00:00Z' }
  if ($State -eq 'released') { $r['released_at'] = '2026-11-21T00:10:00Z' }
  $r['driver'] = 'ec2'
  $r
}
function Set-HpLeases($H, $Records) {
  Write-HpFile -H $H -Path (Get-HpLeasesFile $H) -Text ((ConvertTo-Json -InputObject ([ordered]@{ version = 0; leases = @($Records) }) -Compress -Depth 8) + "`n")
}
function Read-HpLeases($H) {
  $f = Get-HpLeasesFile $H
  if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { return $null }
  try { (ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($f)) -AsHashtable -DateKind String).leases } catch { $null }
}

# ---- Running the operator ---------------------------------------------------------------------------
# The environment of one run: the three seam variables, the zone, no ProxyCommand override unless the
# case gives one, and a PATH that finds no `go` unless $WithGo (a run with $WithGo builds with the `go`
# stand-in, and only when C:\Projects\localswim exists: see Invoke-HpInHome -NeedsLocalswim), with only
# Windows' own folder on it.
function Get-HpEnvironment {
  param([Parameter(Mandatory)]$S, [switch]$WithGo, [hashtable]$Extra = @{})
  $sys = Join-Path $env:SystemRoot 'System32'
  $path = if ($WithGo) { $S.GoDir + ';' + $sys } else { $sys }
  $e = @{ AGENTSPACE_AWS_CMD = $S.aws; AGENTSPACE_SSH_CMD = $S.ssh; AGENTSPACE_SCP_CMD = $S.scp; AGENTSPACE_TIMEZONE = 'America/New_York'; AGENTSPACE_PROXY_COMMAND = $null; PATH = $path }
  foreach ($k in $Extra.Keys) { $e[$k] = $Extra[$k] }
  $e
}
function Invoke-HpOperator {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [Parameter(Mandatory)][string[]]$Arguments, [switch]$WithGo, [hashtable]$Extra = @{}, [int]$Timeout = 300)
  Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $H -Arguments $Arguments -Environment (Get-HpEnvironment -S $S -WithGo:$WithGo -Extra $Extra) -TimeoutSeconds $Timeout
}
# Starts the operator in a thread job, through the harness, so that the check can act at a hook while
# it runs. Receive it with Wait-HpOperatorJob.
function Start-HpOperatorJob {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [Parameter(Mandatory)][string[]]$Arguments, [switch]$WithGo, [hashtable]$Extra = @{}, [int]$Timeout = 300)
  $envir = Get-HpEnvironment -S $S -WithGo:$WithGo -Extra $Extra
  Start-ThreadJob -ArgumentList $script:HpHelpers, $H, $Arguments, $envir, $Timeout -ScriptBlock {
    param($helpers, $home1, $args1, $env1, $timeout)
    . $helpers
    Invoke-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD' -ConformanceHome $home1 -Arguments $args1 -Environment $env1 -TimeoutSeconds $timeout
  }
}
function Wait-HpOperatorJob($Job) {
  $r = $Job | Wait-Job | Receive-Job
  $Job | Remove-Job -Force
  $r
}
# Waits for a hook's stand-in to reach it (up to $Seconds), or for the run's job to end; returns whether
# the hook was reached. A build that never reaches the hook (the v0.06 operator, which has no provision)
# ends its job at once, so the check never waits out the limit.
function Wait-HpHook {
  param([Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$Name, $Job, [int]$Seconds = 240)
  $flag = Join-Path $S.Dir "hook-$Name.reached"
  $until = [DateTime]::UtcNow.AddSeconds($Seconds)
  while (-not (Test-Path -LiteralPath $flag) -and [DateTime]::UtcNow -lt $until) {
    if ($Job -and $Job.State -in 'Completed', 'Failed', 'Stopped') { break }
    Start-Sleep -Milliseconds 100
  }
  Test-Path -LiteralPath $flag
}
function Release-HpHook {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)]$S, [Parameter(Mandatory)][string]$Name)
  Write-HpFile -H $H -Path (Join-Path $S.Dir "hook-$Name.go") -Text 'x'
}

# ---- What the stand-ins saw -------------------------------------------------------------------------
# Every call in order: n, kind, ticks, args, alias, class, paths, opdirs, lockHeld, cwd and goenv.
function Get-HpCalls($S) {
  $log = Join-Path $S.Dir 'calls.jsonl'
  if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { return @() }
  @(Get-Content -LiteralPath $log | Where-Object { $_.Trim() -ne '' } | ForEach-Object { ConvertFrom-Json -InputObject $_ -AsHashtable })
}
# The name a call goes by: `aws <service> <operation>`, `ssh`, `scp`, `go`.
function Get-HpCallName($Call) {
  if ($Call['kind'] -eq 'aws') { return 'aws ' + (($Call['a'] | Select-Object -First 2) -join ' ') }
  $Call['kind']
}
function Get-HpStdin($S, [int]$N) {
  $f = Join-Path $S.Dir "stdin\$N.bin"
  if (Test-Path -LiteralPath $f -PathType Leaf) { [IO.File]::ReadAllBytes($f) } else { $null }
}
function Get-HpSnapshotText($S, [int]$N) {
  $f = Join-Path $S.Dir "snap\$N.json"
  if (Test-Path -LiteralPath $f -PathType Leaf) { [IO.File]::ReadAllText($f) } else { $null }
}

# The lines of the provisioning log: each parsed into Time, Step, Outcome and the rest. $Raw holds the
# lines as written, and $Bytes the file's bytes (or $null when there is no file).
function Read-HpLog($H, [string]$Project) {
  $f = Get-HpLogFile $H $Project
  $bytes = Get-HpFileBytes $f
  $lines = @()
  if ($null -ne $bytes) {
    $text = [Text.UTF8Encoding]::new($false).GetString($bytes)
    $lines = @($text -split "`n" | Where-Object { $_ -ne '' })
  }
  $parsed = @(foreach ($l in $lines) {
      $m = [regex]::Match($l, '^(?<t>\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (?<step>[a-z]+) (?<out>[a-z]+)(?<rest>.*)$')
      [pscustomobject]@{ Raw = $l; Ok = $m.Success; Time = $m.Groups['t'].Value; Step = $m.Groups['step'].Value; Outcome = $m.Groups['out'].Value; Rest = $m.Groups['rest'].Value }
    })
  [pscustomobject]@{ Bytes = $bytes; Lines = $parsed }
}
# `<step> <outcome>` of each line, joined by commas, with a `place done placed=<n>` kept whole.
function Get-HpLogShape($Log) {
  (@($Log.Lines | ForEach-Object { if ($_.Ok) { "$($_.Step) $($_.Outcome)" + $(if ($_.Step -eq 'place' -and $_.Rest -match '^ placed=\d+$') { $_.Rest } else { '' }) } else { '?' + $_.Raw } })) -join ','
}

# The stderr of a run split at line feeds, the last line feed ending no further line.
function Get-HpLines([string]$Text) {
  if ($Text.Length -eq 0) { return , @() }
  if ($Text.EndsWith("`n")) { $Text = $Text.Substring(0, $Text.Length - 1) }
  , @($Text -split "`n")
}

# ---- Reporting ---------------------------------------------------------------------------------------
function New-HpProblems { , [System.Collections.Generic.List[string]]::new() }
function Write-HpProblems([string]$Name, $Problems) {
  if ($Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}
function Add-HpRunProblems($Problems, $Run, [int]$ExitCode) {
  if ($Run.TimedOut) { $Problems.Add('It timed out') }
  elseif ($Run.ExitCode -ne $ExitCode) { $Problems.Add("Exit $($Run.ExitCode), not $ExitCode") }
  if ($Run.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $Problems.Add('Stderr shows a runtime error record') }
}
# The UTC time form of the registry and the log.
$script:HpTimeForm = '^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])T([01]\d|2[0-3]):[0-5]\d:[0-5]\dZ$'

# ---- Shared checks ------------------------------------------------------------------------------------
# True when the gate and the program allow the file's checks to run; otherwise writes the one result.
function Test-HpReady {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)][string]$Name)
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  # The AWS gate runs once per file: it probes every AWS CLI the operator can run (docs/conformance.md, The harness).
  if ($null -eq $script:HpGate) { $script:HpGate = Test-AwsUnreachable -ConformanceHome $H }
  if (-not $script:HpGate.Unreachable) { Write-ConformanceResult -Name $Name -Status SKIPPED -Reason $script:HpGate.Reason; return $false }
  if (-not $program) { Write-ConformanceResult -Name $Name -Status FAIL -Reason 'Neither AGENTSPACE_OPERATOR_CMD nor its default names a program'; return $false }
  if (-not $H.CaReady) { Write-ConformanceResult -Name $Name -Status FAIL -Reason "The harness made no throwaway CA ($($H.CaReason))"; return $false }
  $true
}
$script:HpGate = $null
# Makes a throwaway home and the stand-ins, runs $Body with them, and removes the home in a finally. An
# unexpected error is the case's FAIL, named $Name. A home for each case keeps every case independent.
# A case whose run builds the localswim CLI (a run with -WithGo) MUST pass -NeedsLocalswim: sync's step 7
# builds only when the localswim checkout exists on the machine (docs/operator.md, sync, step 7), so with
# no such folder the case reports SKIPPED, naming the folder. The check is one Test-Path -PathType
# Container on the folder and nothing more: it never lists, reads or writes anything in it.
$script:HpLocalswimCheckout = 'C:\Projects\localswim'
function Test-HpLocalswimFolder { Test-Path -LiteralPath $script:HpLocalswimCheckout -PathType Container }
function Invoke-HpInHome {
  param([Parameter(Mandatory)][scriptblock]$Body, [Parameter(Mandatory)][string]$Name, [switch]$NeedsLocalswim)
  if ($NeedsLocalswim -and -not (Test-HpLocalswimFolder)) {
    Write-ConformanceResult -Name $Name -Status SKIPPED -Reason "The folder $($script:HpLocalswimCheckout) does not exist on this machine, so sync's step 7 builds nothing and a case that shows the build cannot run"
    return
  }
  $h = New-ConformanceHome
  try {
    if (-not (Test-HpReady -H $h -Name $Name)) { return }
    & $Body $h (New-HpStandIns $h)
  } catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    Remove-ConformanceHome -ConformanceHome $h
  }
}
# The files of the repository's box\system as relative path to SHA-256, and box\bootstrap.sh's bytes.
function Get-HpRepoSystemMap {
  $root = Join-Path $script:HpRepo 'box\system'
  $map = @{}
  foreach ($f in Get-ChildItem -LiteralPath $root -Recurse -File -Force) {
    $map[$f.FullName.Substring($root.TrimEnd('\').Length + 1).Replace('\', '/')] = [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData([IO.File]::ReadAllBytes($f.FullName))).Replace('-', '').ToLowerInvariant()
  }
  $map
}
function Get-HpBootstrapBytes { [IO.File]::ReadAllBytes((Join-Path $script:HpRepo 'box\bootstrap.sh')) }
# Two path texts name the same place, without regard to case.
function Test-HpSamePath([string]$A, [string]$B) {
  if ([string]::IsNullOrEmpty($A) -or [string]::IsNullOrEmpty($B)) { return $false }
  [IO.Path]::GetFullPath($A).TrimEnd('\') -ieq [IO.Path]::GetFullPath($B).TrimEnd('\')
}
# The 32-hex folder name under the operator's folder that a path argument names, or $null.
function Get-HpOperatorHex($H, [string]$Path) {
  $root = (Join-Path $H.UserProfile '.agentspace\operator').TrimEnd('\') + '\'
  if ($Path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
    $first = ($Path.Substring($root.Length) -split '\\')[0]
    if ($first -cmatch '^[0-9a-f]{32}$') { return $first }
  }
  $null
}
# The 32-hex folders that exist under the operator's folder now.
function Get-HpOperatorFolders($H) {
  $root = Join-Path $H.UserProfile '.agentspace\operator'
  if (-not (Test-Path -LiteralPath $root -PathType Container)) { return @() }
  @(Get-ChildItem -LiteralPath $root -Directory -Force | Where-Object { $_.Name -cmatch '^[0-9a-f]{32}$' } | ForEach-Object { $_.Name })
}
# True when a file can be opened for read with no sharing, that is, nobody holds it.
function Test-HpFreeLock([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $true }
  try { $fs = [IO.File]::Open($Path, 'Open', 'Read', 'None'); $fs.Dispose(); $true } catch { $false }
}
# Holds a lock file as the operator does: the open handle with no sharing is the lock. Returns the stream.
function Hold-HpLock {
  param([Parameter(Mandatory)]$H, [Parameter(Mandatory)][string]$Path)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $H -Path (Split-Path -Parent $Path)) | Out-Null
  [IO.File]::Open((Assert-ConformancePath -ConformanceHome $H -Path $Path), 'OpenOrCreate', 'ReadWrite', 'None')
}
# The AWS answers of a healthy run for a host that is not the box: the stack's existence check and its
# InstanceId output, the deploy (which writes a marker to stdout), and a running instance launched long ago.
function Get-HpHostAws {
  param([string]$Project = 'agentspace', [string]$Id = $script:HpHostId, [switch]$Exists, [string]$Status = 'UPDATE_COMPLETE')
  $stack = "agentspace-host-$Project"
  $ds = if ($Exists) { @((New-HpStack $stack $Status $Id)) } else { @((New-HpNoStack $stack), (New-HpStack $stack 'CREATE_COMPLETE' $Id)) }
  @{ 'cloudformation describe-stacks' = $ds; 'cloudformation deploy' = @((New-HpStep "deploy-out`n")); 'ec2 describe-instances' = @((New-HpDescribe)) }
}
# The bootstrap's ssh writes a marker to stdout.
function Get-HpBootstrapRule { @{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n" } }
# The names of every call in order, with an ssh as ssh:<class>.
function Get-HpSequence($Calls) {
  @($Calls | ForEach-Object { if ($_['kind'] -eq 'ssh') { 'ssh:' + $_['class'] } else { Get-HpCallName $_ } })
}
# The AWS calls of `sync`'s patch wait for the host whose instance is $Id, as the argument lists the stand-in
# logs, in the order of docs/operator.md, Calls by command, and The patch wait: one `ssm
# describe-instance-information`, one `ssm send-command` with the comment `agentspace sync: patch check`, and
# one `ssm get-command-invocation` of the command the send answered, which with the stand-in's default
# answers is the command ID of zeros. The send's question is the box's own wording, so its value is `commands=`
# and then anything.
$script:HpPatchWaitCommandId = '00000000-0000-0000-0000-000000000000'
$script:HpPatchWaitSequence = @('aws ssm describe-instance-information', 'aws ssm send-command', 'aws ssm get-command-invocation')
function Get-HpPatchWaitCalls([string]$Id) {
  $calls = [System.Collections.Generic.List[object]]::new()
  $calls.Add([string[]]@('ssm', 'describe-instance-information', '--region', 'us-west-2', '--output', 'json', '--filters', "Key=InstanceIds,Values=$Id"))
  $calls.Add([string[]]@('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $Id, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace sync: patch check', '--parameters', 're:^commands='))
  $calls.Add([string[]]@('ssm', 'get-command-invocation', '--region', 'us-west-2', '--output', 'json', '--command-id', $script:HpPatchWaitCommandId, '--instance-id', $Id))
  $calls.ToArray()
}
# Problems found in the `ssm` calls among $Calls (a run's calls) for a run whose `sync` waited once, for the host
# whose instance is $Id, and whose host answered the stand-in's defaults at once: none means exactly the three
# calls of Get-HpPatchWaitCalls, in that order.
function Get-HpPatchWaitProblems($Calls, [string]$Id) {
  $problems = [System.Collections.Generic.List[string]]::new()
  $ssm = @($Calls | Where-Object { $_['kind'] -eq 'aws' -and $_['a'][0] -ceq 'ssm' })
  $want = @(Get-HpPatchWaitCalls $Id)
  if ($ssm.Count -ne 3) { $problems.Add("There were $($ssm.Count) ssm calls, not the patch wait's 3"); return $problems.ToArray() }
  for ($i = 0; $i -lt 3; $i++) {
    $got = @($ssm[$i]['a'])
    $w = @($want[$i])
    $ok = $got.Count -eq $w.Count
    for ($j = 0; $ok -and $j -lt $w.Count; $j++) {
      if ($w[$j].StartsWith('re:')) { if ($got[$j] -cnotmatch $w[$j].Substring(3)) { $ok = $false } } elseif ($got[$j] -cne $w[$j]) { $ok = $false }
    }
    if (-not $ok) { $problems.Add("The patch wait's ssm call $($i + 1) is not '$(($w -join ' ') -replace 're:\^commands=', 'commands=<question>')'") }
  }
  $problems.ToArray()
}
# A time in the log's or the registry's form, as a DateTime in UTC, or $null.
function ConvertFrom-HpTime([string]$Text) {
  $t = [DateTime]::MinValue
  if ([DateTime]::TryParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal, [ref]$t)) { return $t }
  $null
}
# True when a time in the form lies from $From minus a second to $To plus a second.
function Test-HpTimeBetween([string]$Text, [DateTime]$From, [DateTime]$To) {
  $t = ConvertFrom-HpTime $Text
  if ($null -eq $t) { return $false }
  $t -ge $From.AddSeconds(-1.5) -and $t -le $To.AddSeconds(1.5)
}
# The path of a host's or a lease's alias file in the home's ssh folder.
function Get-HpAliasFile($H, [string]$Alias) { Join-Path $H.SshDir "agentspace.d\$Alias" }

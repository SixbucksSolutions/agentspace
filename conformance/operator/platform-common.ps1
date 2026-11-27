#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code for v0.12's operator checks of the shared and Linux parts, test-unmet-platform-*.ps1 in
# conformance/operator/ (E0.12-T04-S02, card #0262; docs/conformance.md, "v0.12's planned cases for
# the shared and Linux parts"). It loads helpers.ps1, and so the harness, which starts every program
# and makes every home, when no common file has done so. This file starts no program under test
# itself and loads no program's code.
#
# What it gives: the names of a host by project and platform (docs/operator.md, "A host's platform
# and its names"), a host record with its `platform` member, the files a signing leaves for a host
# of a platform, a snapshot of a throwaway home's files, and a capture of one run, so that two runs
# of the same command line can be compared. Every function's name has the prefix Pf, and none is
# the name of an alias (checked with Get-Alias).
#
# SAFETY. Every write and delete this file makes calls Assert-ConformancePath at the call, and it
# reads only the throwaway home. A made-up image ID is built from the all-zero form the identifier
# scan allows, and no value of an environment is read or printed.
# A file loads its own framework's common file first (host-commands-common.ps1, host-guard-common.ps1,
# host-provision-common.ps1 or host-proxy-common.ps1), and then this one, so that helpers.ps1 and the harness
# are loaded once. The functions below that read the fixture ($script:Fx, Reset-HostCase, Invoke-HostRun) need
# host-commands-common.ps1 or a file that loads it; the names and the records need nothing.
if (-not (Get-Command Get-OperatorAwsPrograms -ErrorAction SilentlyContinue)) { . (Join-Path $PSScriptRoot 'helpers.ps1') }

# The made-up image ID of the stand-in's `ssm get-parameter` answer: the all-zero form, which the
# identifier scan does not count, and which no real image has. A line the operator prints MUST NOT
# hold it (docs/operator.md, deploy, step 3).
$script:PfImage = 'ami' + '-' + ('0' * 17)

# ---- Names (docs/operator.md, "A host's platform and its names") ----

# The name part of a platform: its one `/` written `-`.
function Get-PfPart([string]$Platform) { $Platform.Replace('/', '-') }
# A host's name: the project on linux/arm64, `<project>.<os>-<arch>` otherwise.
function Get-PfHostName([string]$Project, [string]$Platform = 'linux/arm64') {
  if ($Platform -ceq 'linux/arm64') { $Project } else { $Project + '.' + (Get-PfPart $Platform) }
}
# A host's stack. The box is swimcoach's linux/arm64 host alone.
function Get-PfStack([string]$Project, [string]$Platform = 'linux/arm64') {
  if ($Platform -ceq 'linux/arm64') { if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" } }
  else { 'agentspace-' + (Get-PfPart $Platform) + "-host-$Project" }
}
# A host's operator alias and host-key name (never the box's, which has the user's own entry).
function Get-PfAlias([string]$Project, [string]$Platform = 'linux/arm64') { 'agentspace-host.' + (Get-PfHostName $Project $Platform) }

# One record as text, its members in the order of state.md's member table, `platform` after
# `project` and written for a platform other than linux/arm64 alone (pin P16).
function New-PfRecord {
  param([Parameter(Mandatory)][string]$Project, [string]$Platform = 'linux/arm64', [string]$State = 'ready', [string]$InstanceId = '', [string]$InstanceType = 'm9g.xlarge', [switch]$NoInstanceType, [switch]$WithPlatformMember)
  $t = '{"project":"' + $Project + '"'
  if ($Platform -cne 'linux/arm64' -or $WithPlatformMember) { $t += ',"platform":"' + $Platform + '"' }
  $t += ',"stack":"' + (Get-PfStack $Project $Platform) + '","state":"' + $State + '"'
  if ($InstanceId) { $t += ',"instance_id":"' + $InstanceId + '"' }
  if ($State -ceq 'ready' -and -not $NoInstanceType) { $t += ',"instance_type":"' + $InstanceType + '"' }
  $t + '}'
}

# The files a host's signing and its commands leave, under the host's own folder.
function Get-PfFiles([string]$Project, [string]$Platform = 'linux/arm64') {
  $fx = $script:Fx
  $name = Get-PfHostName $Project $Platform
  $dir = Join-Path $fx.HostsDir $name
  [pscustomobject]@{
    Name = $name
    Dir = $dir
    Key = Join-Path $dir 'id_ed25519'
    Cert = Join-Path $dir 'id_ed25519-cert.pub'
    Alias = Join-Path $fx.AliasDir (Get-PfAlias $Project $Platform)
    Stamp = Join-Path $dir 'localswim-pushed.json'
    Log = Join-Path $dir 'provision.log'
    Lock = Join-Path $dir 'provision.lock'
  }
}
# What an operator command that signs for a host leaves: the key, the certificate (its key ID
# `operator-<host name>`), the alias file and the Include line. A list of problems; none means all are there.
function Get-PfSignProblems([string]$Project, [string]$Platform = 'linux/arm64') {
  $f = Get-PfFiles $Project $Platform
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in @(@('the operator''s key', $f.Key), @('the certificate', $f.Cert), @('the alias file', $f.Alias))) {
    if (-not (Test-Path -LiteralPath $x[1] -PathType Leaf)) { $p.Add("$($x[0]) for host '$($f.Name)' was not written") }
  }
  if ($p.Count -eq 0) {
    $kg = Join-Path $env:SystemRoot 'System32\OpenSSH\ssh-keygen.exe'
    $out = (& $kg -L -f $f.Cert 2>&1 | Out-String)
    if ($out -notmatch ('Key ID: "operator-' + [regex]::Escape($f.Name) + '"')) { $p.Add("The certificate's key ID is not operator-$($f.Name)") }
  }
  $p.ToArray()
}

# ---- A snapshot of the throwaway home's files ----

# One line per file under the home's profile folder: its path below it, with a 32-digit hex folder
# name (the operator's own folder) written <hex>, and a hash of its bytes. A file a program writes
# afresh on every run is listed by name alone: a key, a certificate, a provision log, a lock. Time stamps in
# a JSON file are written <time>, so that two runs made seconds apart compare equal. Sorted, so
# that two snapshots compare as text.
function Get-PfSnapshot {
  $fx = $script:Fx
  # The long form of the profile folder, as the listing below spells it (the home's own path can be an 8.3 short one).
  $root = Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.H.UserProfile
  $rows = foreach ($f in Get-ChildItem -LiteralPath $root -Recurse -File -Force -ErrorAction SilentlyContinue) {
    $rel = $f.FullName.Substring($root.Length).TrimStart('\') -replace '[0-9a-f]{32}', '<hex>'
    # PowerShell's own start-up data, which the stand-ins (PowerShell scripts) write under the home's AppData.
    if ($rel -like 'AppData\Local\Microsoft\*') { continue }
    if ($f.Name -in @('id_ed25519', 'id_ed25519.pub', 'id_ed25519-cert.pub', 'provision.log', 'provision.lock', 'hosts.lock')) { "$rel  (by name)"; continue }
    $bytes = [IO.File]::ReadAllBytes($f.FullName)
    if ($f.Extension -ceq '.json') {
      $text = [Text.Encoding]::UTF8.GetString($bytes) -replace '\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d+)?Z', '<time>'
      $bytes = [Text.Encoding]::UTF8.GetBytes($text)
    }
    "$rel  " + [BitConverter]::ToString([Security.Cryptography.SHA256]::HashData($bytes)).Replace('-', '')
  }
  (@($rows) | Sort-Object { $_ } -CaseSensitive) -join "`n"
}

# ---- A capture of one run, and the comparison of two ----

# A call's arguments as text, a 32-digit hex folder written <hex>, one line per call.
function Get-PfCallLines($Calls) { @($Calls | ForEach-Object { ($_.a -join ' ') -replace '[0-9a-f]{32}', '<hex>' }) }
# Runs the operator once, after the setup, and returns what a comparison needs: the exit code, both
# streams, the stand-ins' calls and the home's files. Nothing is checked here.
function Invoke-PfCapture {
  param([Parameter(Mandatory)][string[]]$Arguments, [scriptblock]$Setup = {}, [hashtable]$Environment = @{}, [int]$Timeout = 180)
  Reset-HostCase
  & $Setup
  $r = Invoke-HostRun -Arguments $Arguments -Environment $Environment -Timeout $Timeout
  [pscustomobject]@{
    TimedOut = $r.TimedOut
    Exit = $r.ExitCode
    Stdout = ($r.Stdout -replace '[0-9a-f]{32}', '<hex>')
    Stderr = ($r.Stderr -replace '[0-9a-f]{32}', '<hex>')
    Aws = (Get-PfCallLines (Get-AwsCalls))
    Ssh = (Get-PfCallLines (Get-SshCalls))
    Scp = (Get-PfCallLines (Get-ScpCalls))
    Files = (Get-PfSnapshot)
  }
}
# The problems between two captures, each named by what differs, never quoting a program's output.
function Get-PfDifference($A, $B) {
  $p = [System.Collections.Generic.List[string]]::new()
  if ($A.TimedOut -or $B.TimedOut) { $p.Add('A run timed out') }
  if ($A.Exit -ne $B.Exit) { $p.Add("The exit codes differ ($($A.Exit) and $($B.Exit))") }
  if ($A.Stdout -cne $B.Stdout) { $p.Add('Stdout differs') }
  if ($A.Stderr -cne $B.Stderr) { $p.Add('Stderr differs') }
  foreach ($k in 'Aws', 'Ssh', 'Scp') {
    $x = @($A.$k); $y = @($B.$k)
    if ($x.Count -ne $y.Count) { $p.Add("The $k calls differ in number ($($x.Count) and $($y.Count))") }
    else { for ($i = 0; $i -lt $x.Count; $i++) { if ($x[$i] -cne $y[$i]) { $p.Add("The $k call $($i + 1) differs"); break } } }
  }
  if ($A.Files -cne $B.Files) {
    # The paths that differ, never their bytes: a path below the profile folder is a name of the spec's.
    $x = @($A.Files -split "`n"); $y = @($B.Files -split "`n")
    $names = @(@($x | Where-Object { $_ -notin $y }) + @($y | Where-Object { $_ -notin $x }) | ForEach-Object { ($_ -split '  ')[0] } | Sort-Object -Unique)
    $p.Add('The files the run left differ: ' + ($names -join ', '))
  }
  $p.ToArray()
}

# True when two spellings name one path: as given, or in the long form the harness returns.
function Test-PfSamePath([string]$Text, [string]$Path) {
  $Text -ieq $Path -or $Text -ieq (Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $Path)
}

# Joins the problems of a list into one result: PASS when empty, else FAIL with the reason masked.
function Write-PfResult([string]$Name, $Problems) {
  if (@($Problems).Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked (@($Problems) -join '; ')) }
}
# Runs a case's body under the file's usual error handling: an error in the check is a FAIL result
# naming the line, never a crash of the file.
function Invoke-PfCase([string]$Name, [scriptblock]$Body) {
  try {
    $p = [System.Collections.Generic.List[string]]::new()
    & $Body $p
    Write-PfResult $Name $p
  } catch {
    $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "The check hit an error: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  }
}

# ---- Times and leases ----

# The contract's form, `YYYY-MM-DDTHH:MM:SSZ`, as the lease state file and the host registry hold it.
function Format-PfTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
# The form the AWS CLI prints a LaunchTime in.
function Format-PfAwsTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'+00:00'", [Globalization.CultureInfo]::InvariantCulture) }
# Now in UTC, truncated to the whole second, and a time this many minutes before it, with a nonzero second.
function Get-PfNow { $t = [DateTime]::UtcNow; [DateTime]::new($t.Ticks - ($t.Ticks % 10000000), [DateTimeKind]::Utc) }
function Get-PfAgo([double]$Minutes, [ValidateRange(1, 59)][int]$Second = 37) { $b = (Get-PfNow).AddMinutes(-$Minutes); $b.AddSeconds($Second - $b.Second) }
function Get-PfAhead([double]$Minutes, [ValidateRange(1, 59)][int]$Second = 37) { Get-PfAgo -Minutes (-$Minutes) -Second $Second }
# `<h:mm><am|pm>` in the laptop's zone, read from the system now, the seconds rounded up to the next whole minute
# (docs/operator.md, status -All and guard). The caller never gives a time of a daylight saving change.
function Format-PfClock([DateTime]$Utc) {
  $t = $Utc.ToUniversalTime()
  if ($t.Second -gt 0 -or $t.Millisecond -gt 0) { $t = $t.AddSeconds(60 - $t.Second).AddMilliseconds(-$t.Millisecond) }
  [TimeZoneInfo]::ClearCachedData()
  $l = [TimeZoneInfo]::ConvertTimeFromUtc($t, [TimeZoneInfo]::Local)
  $l.ToString('h:mmtt', [Globalization.CultureInfo]::InvariantCulture).ToLowerInvariant()
}

$script:PfLeaseN = 0
# One record of the lease state file, as text, of a project on a platform. A ready record needs -Expires; a
# released one -Released, and -Expires too when the lease was ever ready. The lease's alias is
# agentspace-<project>-<first 8 hex digits of its ID> on every platform (pin P7).
function New-PfLease {
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State,
    [string]$Platform = 'linux/arm64',
    [Nullable[DateTime]]$Expires = $null,
    [Nullable[DateTime]]$Released = $null,
    [string]$Id = ''
  )
  $script:PfLeaseN++
  if (-not $Id) { $Id = ('{0:x8}-0000-4000-8000-{1:x12}' -f (0xabc00000 + $script:PfLeaseN), $script:PfLeaseN) }
  $r = [ordered]@{ id = $Id; project = $Project; state = $State; platform = $Platform; profile = 'agent-linux@3'; size = 'm'; ttl_seconds = 3600; created_at = '2026-01-01T00:00:00Z' }
  if ($null -ne $Expires) { $r.expires_at = Format-PfTime $Expires }
  if ($null -ne $Released) { $r.released_at = Format-PfTime $Released }
  if ($State -eq 'ready') { $r.ssh = [ordered]@{ host_alias = ('agentspace-{0}-{1}' -f $Project, $Id.Substring(0, 8)); user = 'ubuntu' } }
  if ($State -eq 'unsatisfiable') { $r.reasons = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
  $r.driver = 'ec2'
  ConvertTo-Json -Compress -Depth 6 -InputObject $r
}
# The alias a lease record text gives its connection.
function Get-PfLeaseAlias([string]$Record) { (ConvertFrom-Json -InputObject $Record).ssh.host_alias }
# Writes the lease state file as the lease CLI writes it, into the case's state folder. The records MUST validate
# against the lease state schema, or this throws, since a bad record is the check's mistake.
function Set-PfLeases([string[]]$Record = @()) {
  $fx = $script:Fx
  $text = '{"version":0,"leases":[' + ($Record -join ',') + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text -Schema 'state/leases.state.schema.json')) { throw 'The seeded lease state file does not validate against its schema.' }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.H.StateDir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.H.StateDir 'leases.json')), [Text.UTF8Encoding]::new($false).GetBytes($text))
}

# ---- A `go` stand-in ----

# A `go` that logs its arguments and the three Go variables it sees (never an environment), writes the file
# `-o` names when that is under the home's profile folder, and exits 0. It is a .cmd in the fixture's
# empty PATH folder, which Initialize-HostFixture puts first on the operator's PATH, so the operator finds it
# and no run starts the real `go`. Its log, go-args.log, is in the stand-ins' folder, which Reset-HostCase clears.
$script:PfGoScript = @'
$dir = $env:PF_GO_LOGDIR
$a = @($args | ForEach-Object { [string]$_ })
$entry = [ordered]@{ a = $a; goos = [string]$env:GOOS; goarch = [string]$env:GOARCH; cgo = [string]$env:CGO_ENABLED }
[IO.File]::AppendAllText((Join-Path $dir 'go-args.log'), ((ConvertTo-Json -Compress -InputObject $entry) + "`n"))
$i = [Array]::IndexOf($a, '-o')
if ($i -ge 0 -and $i + 1 -lt $a.Count) {
  $out = [IO.Path]::GetFullPath($a[$i + 1])
  $root = [IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd('\') + '\'
  if ($out.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { [IO.File]::WriteAllText($out, 'fake localswim build') }
}
exit 0
'@
function New-PfGoStandIn {
  $fx = $script:Fx
  $pwsh = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
  $ps1 = Join-Path $fx.Standins 'go-standin.ps1'
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $ps1), $script:PfGoScript)
  $bin = Join-Path $fx.H.Root 'emptybin'
  $cmd = Join-Path $bin 'go.cmd'
  Set-Content -LiteralPath (Assert-ConformancePath -ConformanceHome $fx.H -Path $cmd) -Encoding ascii -Value @('@echo off', ('"' + $pwsh + '" -NoProfile -File "' + $ps1 + '" %*'), 'exit /b %ERRORLEVEL%')
  # The log folder reaches the stand-in by name, since a .cmd cannot be given the stand-ins' folder otherwise.
  $fx.Env['PF_GO_LOGDIR'] = $fx.Standins
}
function Get-PfGoCalls { Read-JsonLog (Join-Path $script:Fx.Standins 'go-args.log') }

# The calls of one program, one string each, for a search by text.
function Get-PfCallTexts($Calls) { @($Calls | ForEach-Object { $_.Text }) }

# Runs the operator once after a setup and reports one result from a check block alone, for a case
# whose exit code, streams and calls the spec does not fix (a command that goes on to a step whose
# stand-in answers the case does not model). The check takes the run and its start, and returns
# problems; a run that timed out, or whose stderr shows a runtime error record, fails whatever it returns.
function Test-PfRun {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][AllowEmptyString()][string[]]$Arguments,
    [scriptblock]$Setup = {},
    [Parameter(Mandatory)][scriptblock]$Check,
    [hashtable]$Environment = @{},
    [int]$Timeout = 180
  )
  Invoke-PfCase $Name {
    param($p)
    Reset-HostCase
    & $Setup
    $r = Invoke-HostRun -Arguments $Arguments -Environment $Environment -Timeout $Timeout
    if ($r.TimedOut) { $p.Add('It timed out') }
    if ($r.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $p.Add('Stderr shows a runtime error record') }
    foreach ($m in @(& $Check $r $r.Started)) { if ($m) { $p.Add([string]$m) } }
  }
}

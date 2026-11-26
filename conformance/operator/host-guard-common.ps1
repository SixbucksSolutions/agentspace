#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Shared code for the conformance check of `agentspace-operator guard` (docs/operator.md, "guard";
# docs/state.md, "The guard's files", "Live leases and the lease horizon"; D56, D57, D63):
# test-unmet-guard.ps1 in conformance/operator/ (E0.08-T04-S01, card #0182). It dot-sources
# host-commands-common.ps1, and so helpers.ps1 and the harness, and reuses its fixture (the stand-ins
# for `aws`, `ssh` and `scp`, which log every call and answer from files beside them), its host
# registry helpers and its case runner. This file starts no program under test itself and loads no
# program's code.
#
# What it adds, for the guard:
#   - Time helpers: the run's clock has no seam (docs/operator.md, guard, "Its seams for conformance"),
#     so a case places every time relative to the real clock, to the whole second, with its own times
#     never within 3 minutes of a decision's boundary, and reads back what the guard decided.
#   - Seeding: the lease state file (pending, ready, released and unsatisfiable records, which it
#     checks against the lease state schema before it writes), the answer of `ec2 describe-instances`
#     for a list of instances, a held `guard.lock` or `provision.lock`, and a folder at a lock's or the
#     log's path.
#   - Expectations: the guard's calls, its progress lines, its closing line and its `guard.log` lines in
#     the exact forms of operator.md, a check of the log's bytes, and a check of the command each
#     `ssm send-command` carries (the horizon as one word, and no comma, bracket or equals sign).
#   - The checks every guard run is held to: it wrote neither the host registry nor the lease state
#     file, took neither `hosts.lock` nor `leases.lock`, started no ssh or scp, made no `operator\`
#     folder, and wrote no key, certificate or alias (the home's CA is moved away for every run).
#
# SAFETY. Every write and delete this file makes calls Assert-ConformancePath at the call, through
# host-commands-common.ps1's helpers or inline (docs/conformance.md, "The program under test is
# untrusted"). It records nothing of any environment. Every identifier is made up.
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$script:GdHolds = [System.Collections.Generic.List[IO.FileStream]]::new()
$script:GdBefore = @{ Hosts = $null; Leases = $null }
$script:GdLeaseN = 0
$script:GdSignIn = "AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$script:GdSignInStandIn = 'Unable to locate credentials. You can configure credentials by running "aws login".'

# ---- Identifiers and times ----

# An obviously made-up instance ID: the digit repeated (1 to 9), `i-11111111111111111` and so on.
function Get-GdId([ValidateRange(1, 9)][int]$N) { 'i-' + ([string]$N * 17) }

# Now, in UTC, truncated to the whole second.
function Get-GdNow { $t = [DateTime]::UtcNow; [DateTime]::new($t.Ticks - ($t.Ticks % 10000000), [DateTimeKind]::Utc) }
# A time this many minutes before now (or after it, when negative), its seconds set to a nonzero
# value so that a stop time always has seconds, and so a progress line's rounding up is certain.
function Get-GdAgo([double]$Minutes, [ValidateRange(1, 59)][int]$Second = 37) {
  $b = (Get-GdNow).AddMinutes(-$Minutes)
  $b.AddSeconds($Second - $b.Second)
}
function Get-GdAhead([double]$Minutes, [ValidateRange(1, 59)][int]$Second = 37) { Get-GdAgo -Minutes (-$Minutes) -Second $Second }
# The contract's form, `YYYY-MM-DDTHH:MM:SSZ`, as the lease state file, the host registry and guard.log hold it.
function Format-GdTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
# The form the AWS CLI prints a LaunchTime in: with an offset, here UTC's.
function Format-GdAwsTime([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'+00:00'", [Globalization.CultureInfo]::InvariantCulture) }
function Get-GdUnix([DateTime]$T) { [DateTimeOffset]::new($T.ToUniversalTime()).ToUnixTimeSeconds() }
# `<h:mm><am|pm>` in the laptop's zone, read from the system now, the seconds rounded up to the next whole
# minute (docs/operator.md, status -All). The caller never gives a time of a daylight saving change.
function Get-GdClock([DateTime]$Utc) {
  $t = $Utc.ToUniversalTime()
  if ($t.Second -gt 0 -or $t.Millisecond -gt 0) { $t = $t.AddSeconds(60 - $t.Second).AddMilliseconds(-$t.Millisecond) }
  [TimeZoneInfo]::ClearCachedData()
  $l = [TimeZoneInfo]::ConvertTimeFromUtc($t, [TimeZoneInfo]::Local)
  $l.ToString('h:mmtt', [Globalization.CultureInfo]::InvariantCulture).ToLowerInvariant()
}

# ---- The lease state file ----

# One record of the lease state file, as text. A ready record needs -Expires; a released one -Released, and
# -Expires too when the lease was ever ready; the others take neither.
function New-GdLease {
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State,
    [Nullable[DateTime]]$Expires = $null,
    [Nullable[DateTime]]$Released = $null,
    [string]$Id = ''
  )
  $script:GdLeaseN++
  if (-not $Id) { $Id = ('{0:x8}-0000-4000-8000-{1:x12}' -f $script:GdLeaseN, $script:GdLeaseN) }
  $r = [ordered]@{ id = $Id; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux@3'; size = 'm'; ttl_seconds = 3600; created_at = '2026-01-01T00:00:00Z' }
  if ($null -ne $Expires) { $r.expires_at = Format-GdTime $Expires }
  if ($null -ne $Released) { $r.released_at = Format-GdTime $Released }
  if ($State -eq 'ready') { $r.ssh = [ordered]@{ host_alias = ('agentspace-{0}-{1:x8}' -f $Project, $script:GdLeaseN); user = 'ubuntu' } }
  if ($State -eq 'unsatisfiable') { $r.reasons = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
  $r.driver = 'ec2'
  ConvertTo-Json -Compress -Depth 6 -InputObject $r
}
# Writes the lease state file as the lease CLI writes it, into the case's state folder or into -Dir. The records
# MUST validate against the lease state schema, or this throws, since a bad record is the check's mistake.
function Set-GdLeases {
  param([string[]]$Record = @(), [string]$Dir = '')
  $fx = $script:Fx
  if (-not $Dir) { $Dir = $fx.H.StateDir }
  $text = '{"version":0,"leases":[' + ($Record -join ',') + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text -Schema 'state/leases.state.schema.json')) { throw 'The seeded lease state file does not validate against its schema.' }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $Dir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $Dir 'leases.json')), [Text.UTF8Encoding]::new($false).GetBytes($text))
}
# Writes text to the lease state file as it is, for a file that is not valid.
function Set-GdLeasesText([string]$Text) {
  $fx = $script:Fx
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.H.StateDir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.H.StateDir 'leases.json')), [Text.UTF8Encoding]::new($false).GetBytes($Text))
}
function Get-GdLeasesPath { Join-Path $script:Fx.H.StateDir 'leases.json' }

# ---- The answers of `ec2 describe-instances` ----

# One instance of the answer. -Launch is a time the AWS CLI prints as UTC with an offset; -LaunchText is the member's
# text as it is; -NoLaunch leaves `LaunchTime` out. With none of the three, the instance was launched 10 hours ago.
function New-GdInstance {
  param([Parameter(Mandatory)][string]$Id, [string]$State = 'running', [Nullable[DateTime]]$Launch = $null, [string]$LaunchText = '', [switch]$NoLaunch)
  $o = [ordered]@{ InstanceId = $Id; State = [ordered]@{ Name = $State } }
  if (-not $NoLaunch) {
    $o.LaunchTime = if ($LaunchText) { $LaunchText } elseif ($null -ne $Launch) { Format-GdAwsTime $Launch } else { Format-GdAwsTime (Get-GdAgo 600) }
  }
  $o.InstanceType = 'm9g.xlarge'
  $o
}
# The answer for a list of instances, each in a reservation of its own, with two elements the guard MUST ignore:
# an instance of no host of the registry, and an element with no `InstanceId`.
function New-GdDescribeDoc {
  param([Parameter(Mandatory)][object[]]$Instance)
  $res = [System.Collections.Generic.List[object]]::new()
  foreach ($i in $Instance) { $res.Add([ordered]@{ Instances = @($i) }) }
  $res.Add([ordered]@{ Instances = @([ordered]@{ InstanceId = 'i-9f9f9f9f9f9f9f9f9'; State = [ordered]@{ Name = 'running' }; LaunchTime = (Format-GdAwsTime (Get-GdAgo 900)) }, [ordered]@{ State = [ordered]@{ Name = 'running' } }) })
  ConvertTo-Json -Compress -Depth 8 -InputObject ([ordered]@{ Reservations = @($res) })
}
# Sets the stand-in's answers: the describe answer for -Instance (a list of New-GdInstance), and -More, a scenario
# of keys of its own (a stack, a stop or a send that fails), which replaces a key of the same name.
function Set-GdAws {
  param([object[]]$Instance = @(), [hashtable]$More = @{})
  $s = @{}
  if (@($Instance).Count -gt 0) { $s['ec2 describe-instances'] = @(New-Step (New-GdDescribeDoc -Instance $Instance)) }
  Set-AwsScenario (Merge-Scenario $s $More)
}

# ---- The calls ----

# The one `ec2 describe-instances` of a run: its filter is one argument, naming each ID in the registry's order.
function Get-GdCallDescribe([string[]]$Id) { , @('ec2', 'describe-instances', '--region', 'us-west-2', '--output', 'json', '--filters', ('Name=instance-id,Values=' + ($Id -join ','))) }
# One horizon send. The command's own text is checked apart from the call, by Get-GdSendProblems.
function Get-GdCallSend([string]$Id) { , @('ssm', 'send-command', '--region', 'us-west-2', '--output', 'json', '--instance-ids', $Id, '--document-name', 'AWS-RunShellScript', '--comment', 'agentspace guard: lease horizon', '--parameters', 're:^commands=') }

function Get-GdSendCalls { @(Get-AwsCalls | Where-Object { $_.a.Count -ge 2 -and $_.a[0] -ceq 'ssm' -and $_.a[1] -ceq 'send-command' }) }
# The text of the remote command a send carries, after `commands=`, or $null when it has no such argument.
function Get-GdSendCommand($Call) {
  $i = [array]::IndexOf($Call.a, '--parameters')
  if ($i -lt 0 -or $i + 1 -ge $Call.a.Count -or -not $Call.a[$i + 1].StartsWith('commands=')) { return $null }
  $Call.a[$i + 1].Substring('commands='.Length)
}
# What the sends of a run MUST show: one send to each host of -Expected and to no other, each command holding the
# horizon as a word of its own, no comma, bracket or equals sign, and the horizon the expected one. -Expected maps an
# instance ID to a UTC time, or to the string 'run' for a horizon that is the run's time, which a run reads once
# after it starts and before it ends, so any second from the start to the end is right.
function Get-GdSendProblems {
  param([Parameter(Mandatory)][hashtable]$Expected, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][DateTime]$Started)
  $p = [System.Collections.Generic.List[string]]::new()
  $sends = Get-GdSendCalls
  $finish = $Started.AddSeconds([double]$Run.Elapsed)
  foreach ($id in $Expected.Keys) {
    $mine = @($sends | Where-Object { $_.a -contains $id })
    if ($mine.Count -ne 1) { $p.Add("The host $id was sent $($mine.Count) horizons, not 1"); continue }
    $cmd = Get-GdSendCommand $mine[0]
    if ($null -eq $cmd) { $p.Add("The send to $id has no commands= argument"); continue }
    if ($cmd -match '[,\[\]=]') { $p.Add("The command sent to $id holds a comma, a bracket or an equals sign") }
    $words = @([regex]::Matches($cmd, '(?<![0-9A-Za-z_.\-/])\d{9,12}(?![0-9A-Za-z_.\-/])') | ForEach-Object { $_.Value })
    if ($words.Count -ne 1) { $p.Add("The command sent to $id holds $($words.Count) words of 9 to 12 digits, not 1"); continue }
    if ($words[0].StartsWith('0')) { $p.Add("The horizon sent to $id has a leading zero") }
    $got = [long]$words[0]
    if ($Expected[$id] -is [string]) {
      $lo = (Get-GdUnix $Started) - 1; $hi = (Get-GdUnix $finish) + 2
      if ($got -lt $lo -or $got -gt $hi) { $p.Add("The horizon sent to $id is not the run's time: it is $($got - (Get-GdUnix $Started)) seconds from the run's start") }
    } else {
      $want = Get-GdUnix $Expected[$id]
      if ($got -ne $want) { $p.Add("The horizon sent to $id is $($got - $want) seconds from the expected $(Format-GdTime $Expected[$id])") }
    }
  }
  foreach ($c in $sends) {
    $known = $false
    foreach ($id in $Expected.Keys) { if ($c.a -contains $id) { $known = $true } }
    if (-not $known) { $p.Add('A horizon was sent to a host that was not expected to get one') }
  }
  $p.ToArray()
}

# ---- The progress lines, the closing line and the log's lines, in the exact forms of operator.md ----

function Get-GdProgressStopping([string]$Project, [string]$Id) { "Stopping the host of '$Project' ($Id): no live lease explains it." }
function Get-GdProgressUnexplained([string]$Project, [string]$Id, [DateTime]$Since) { "The host of '$Project' ($Id) has no live lease; it stops at $(Get-GdClock $Since.AddMinutes(15))." }
function Get-GdProgressProvisioning([string]$Project, [string]$Id) { "The host of '$Project' ($Id) has no live lease, but a provisioning run is working on it; it is not stopped." }
function Get-GdClosing([int]$Hosts, [int]$Running, [int]$Stopped, [int]$Unexplained) { "Hosts: $Hosts; running $Running, stopped $Stopped, unexplained $Unexplained." }
function Get-GdFailedCount([int]$N, [int]$M) { "agentspace: $N of $M hosts failed." }
function Get-GdHostFailure([string]$Project, [string]$Message) { "agentspace: Host '$Project': $Message" }

# The text of a log line after its time.
function Get-GdLineStopped([string]$Project, [DateTime]$Since) { "host=$Project stopped since=$(Format-GdTime $Since)" }
function Get-GdLineUnexplained([string]$Project, [DateTime]$Since, [switch]$Provisioning) {
  $t = "host=$Project unexplained since=$(Format-GdTime $Since) stops=$(Format-GdTime $Since.AddMinutes(15))"
  if ($Provisioning) { $t += ' provisioning=running' }
  $t
}
function Get-GdLineFailed([string]$Project, [string]$Step, [string]$Reason) { "host=$Project failed step=$Step reason=$Reason" }
function Get-GdLineRunFailed([string]$Step, [string]$Reason) { "failed step=$Step reason=$Reason" }

function Get-GdLogPath { Join-Path $script:Fx.Agentspace 'guard.log' }
function Get-GdLockPath { Join-Path $script:Fx.Agentspace 'guard.lock' }
function Get-GdProvisionLockPath([string]$Project) { Join-Path $script:Fx.Agentspace "hosts\$Project\provision.lock" }

# Masks a wanted line, or a pattern for one, before a failure quotes it. A wanted line for a lock, the host registry or
# the lease state file holds the throwaway home's path, which lies under the profile and so holds the Windows user
# name (AGENTS.md, "A live run on Terry's real setup MUST mask what it reads before it prints it"). The home's root, the
# temp folder and the profile are replaced literally, in their regex-escaped and their plain forms, the suite's mask
# (conformance\mask-common.ps1) is applied, and any path that remains is replaced whole.
function Get-GdMasked([string]$Text) {
  $t = $Text
  $roots = [System.Collections.Generic.List[object]]::new()
  if ($null -ne $script:Fx -and $script:Fx.H.Root) { $roots.Add(@($script:Fx.H.Root, '<home>')) }
  if ($env:TEMP) { $roots.Add(@($env:TEMP, '<temp>')) }
  if ($env:USERPROFILE) { $roots.Add(@($env:USERPROFILE, '<profile>')) }
  foreach ($r in $roots) {
    $t = $t.Replace([regex]::Escape($r[0]), $r[1]).Replace([string]$r[0], $r[1])
  }
  $t = ConvertTo-ConformanceMasked $t
  [regex]::Replace($t, '[A-Za-z]:[\\/][^\s''"]*', '<path>')
}

# Reads the guard's stderr as its non-empty lines, compared with a list of wanted lines, each exact or `re:` and a
# regular expression, matched case-sensitively. Returns problems; the failure names the wanted line, masked, never the
# program's.
function Get-GdStderrProblems {
  param([string]$Stderr, [object[]]$Want)
  $p = [System.Collections.Generic.List[string]]::new()
  $lines = @($Stderr -split "`n" | Where-Object { $_ -ne '' })
  if ($lines.Count -ne @($Want).Count) { $p.Add("Stderr has $($lines.Count) lines, not the spec's $(@($Want).Count)") }
  for ($i = 0; $i -lt [Math]::Min($lines.Count, @($Want).Count); $i++) {
    $w = [string]@($Want)[$i]
    $ok = if ($w.StartsWith('re:')) { $lines[$i] -cmatch $w.Substring(3) } else { $lines[$i] -ceq $w }
    if (-not $ok) { $p.Add("Stderr line $($i + 1) is not '$(Get-GdMasked ($w -replace '\s+', ' '))'"); break }
  }
  $p.ToArray()
}

# The log after a run: nothing when -None (no file at all, which a run with nothing to log never creates), else exactly
# the lines of -Body, in order, each a time of the run (from its start to its end) and one space and then the wanted
# text, which is exact, or `re:` and a regular expression for the text after the time. The file's bytes are those of
# state.md, "The guard's log": UTF-8 with no byte order mark, no carriage return, each line ending in one LF.
function Get-GdLogProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][DateTime]$Started, [string[]]$Body = @(), [switch]$None)
  $p = [System.Collections.Generic.List[string]]::new()
  $path = Get-GdLogPath
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    if (-not $None -and $Body.Count -gt 0) { $p.Add('guard.log was not written') }
    return $p.ToArray()
  }
  if ($None) { $p.Add('guard.log was written'); return $p.ToArray() }
  $bytes = [IO.File]::ReadAllBytes($path)
  if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { $p.Add('guard.log starts with a byte order mark') }
  if ($bytes -contains 13) { $p.Add('guard.log holds a carriage return') }
  if ($bytes.Length -gt 0 -and $bytes[$bytes.Length - 1] -ne 10) { $p.Add('guard.log does not end in a line feed') }
  try { $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes) } catch { $p.Add('guard.log is not valid UTF-8'); return $p.ToArray() }
  $lines = @($text -split "`n")
  if ($lines.Count -gt 0 -and $lines[-1] -eq '') { $lines = @($lines | Select-Object -SkipLast 1) }
  if ($lines.Count -ne $Body.Count) { $p.Add("guard.log has $($lines.Count) lines, not $($Body.Count)") }
  $from = $Started.AddSeconds(-2); $to = $Started.AddSeconds([double]$Run.Elapsed + 2)
  for ($i = 0; $i -lt [Math]::Min($lines.Count, $Body.Count); $i++) {
    $m = [regex]::Match($lines[$i], '^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z) (.*)$')
    if (-not $m.Success) { $p.Add("guard.log line $($i + 1) is not a time in the contract's form, a space and its text"); break }
    $t = [DateTime]::MinValue
    if (-not [DateTime]::TryParseExact($m.Groups[1].Value, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal, [ref]$t)) {
      $p.Add("guard.log line $($i + 1) holds a time that is not real"); break
    }
    if ($t -lt $from -or $t -gt $to) { $p.Add("guard.log line $($i + 1) is timed outside the run") }
    $w = $Body[$i]
    $ok = if ($w.StartsWith('re:')) { $m.Groups[2].Value -cmatch $w.Substring(3) } else { $m.Groups[2].Value -ceq $w }
    if (-not $ok) { $p.Add("guard.log line $($i + 1) text is not '$(Get-GdMasked ($w -replace '\s+', ' '))'"); break }
  }
  $p.ToArray()
}
# The lines of the log, after their time, as the case needs them to compare one with another.
function Get-GdLogTexts {
  $path = Get-GdLogPath
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return @() }
  @([IO.File]::ReadAllLines($path) | Where-Object { $_ -ne '' } | ForEach-Object { $_ -replace '^\S+ ', '' })
}

# ---- Locks and the state of a case ----

# Holds a lock file as a program does: open with no sharing, creating the file and its folder. Closed by Close-GdHolds.
function Hold-GdLock([string]$Path) {
  $fx = $script:Fx
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path (Split-Path -Parent $Path)) | Out-Null
  $s = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $fx.H -Path $Path), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  $script:GdHolds.Add($s)
}
function Close-GdHolds {
  foreach ($s in $script:GdHolds) { $s.Dispose() }
  $script:GdHolds.Clear()
}
# A folder where a file is expected, such as at the log's or a lock's path.
function New-GdFolderAt([string]$Path) {
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $Path) | Out-Null
}
# True when nobody holds the file: it opens with no sharing. A missing file is free.
function Test-GdFree([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $true }
  try { $s = [IO.File]::Open($Path, 'Open', 'Read', 'None'); $s.Dispose(); $true } catch { $false }
}

# Puts the guard's own files back to their start: its log and lock, the lease state file and its lock, and any
# other state folder; and moves the home's CA away, so that every run is made with no CA, and the guard shows
# that it signs nothing and never fails on the CA (docs/operator.md, guard, "What it MUST NOT do").
function Reset-GuardState {
  $fx = $script:Fx
  Close-GdHolds
  foreach ($p in @((Get-GdLogPath), (Get-GdLockPath), (Get-GdLeasesPath), (Join-Path $fx.H.StateDir 'leases.lock'), (Join-Path $fx.H.UserProfile 'other-state'))) { Remove-HomePath $p }
  if (Test-Path -LiteralPath $fx.CaDir) { Hide-Ca }
}
# Remembers what the guard MUST leave as it is: the host registry's bytes and the lease state file's.
function Save-GdSnapshot {
  $fx = $script:Fx
  $script:GdBefore = @{ Hosts = (Get-FileBytesOrNull $fx.HostsFile); Leases = (Get-FileBytesOrNull (Get-GdLeasesPath)) }
}
# What no guard run does (docs/operator.md, guard, "What it MUST NOT do"; state.md, "The guard's files").
function Get-GdNeverProblems {
  $fx = $script:Fx
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.HostsFile) $script:GdBefore.Hosts)) { $p.Add('hosts.json was changed') }
  if (-not (Test-SameBytes (Get-FileBytesOrNull (Get-GdLeasesPath)) $script:GdBefore.Leases)) { $p.Add('The lease state file was changed') }
  if (Test-Path -LiteralPath $fx.OperatorDir) { $p.Add('An operator folder was made') }
  if (Test-Path -LiteralPath $fx.HostsLock) { $p.Add('hosts.lock was made') }
  if (Test-Path -LiteralPath (Join-Path $fx.H.StateDir 'leases.lock')) { $p.Add('leases.lock was made') }
  if (Test-Path -LiteralPath $fx.HostsDir) {
    if (@(Get-ChildItem -LiteralPath $fx.HostsDir -Recurse -File -Force -Filter 'id_ed25519*' -ErrorAction SilentlyContinue).Count -gt 0) { $p.Add('A key or a certificate was written for a host') }
  }
  if (Test-Path -LiteralPath $fx.AliasDir) {
    if (@(Get-ChildItem -LiteralPath $fx.AliasDir -Force -ErrorAction SilentlyContinue).Count -gt 0) { $p.Add('An alias file was written') }
  }
  if (Test-Path -LiteralPath (Join-Path $fx.SshDir 'config')) { $p.Add('An ssh config was written') }
  $p.ToArray()
}

# ---- The case runner ----

# Runs `guard` (or the arguments given) once, through host-commands-common.ps1's Test-HostCase, with the guard's own
# files reset first, the CA moved away, and the checks that every guard run is held to added to the case's own.
# Locks the case held are closed when it ends.
function Test-GuardCase {
  param(
    [Parameter(Mandatory)][string]$Name,
    [string[]]$Arguments = @('guard'),
    [Parameter(Mandatory)][hashtable]$Expect,
    [scriptblock]$Setup = {},
    [hashtable]$Environment = @{},
    [int]$Timeout = 120
  )
  $GdUserSetup = $Setup
  $GdUserCheck = $Expect.Check
  $GdExpect = $Expect.Clone()
  $GdExpect.Check = {
    param($run, $started)
    if ($null -ne $GdUserCheck) { foreach ($m in @(& $GdUserCheck $run $started)) { $m } }
    foreach ($m in (Get-GdNeverProblems)) { $m }
  }
  try {
    Test-HostCase -Name $Name -Arguments $Arguments -Expect $GdExpect -Environment $Environment -Timeout $Timeout -Setup {
      Reset-GuardState
      & $GdUserSetup
      Save-GdSnapshot
    }
  } finally {
    Close-GdHolds
  }
}

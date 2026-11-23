#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `status -All`'s lease lines (docs/operator.md, "status", "status -All", "Every
# host at once: -All", "Calls by command"; docs/state.md, "Live leases and the lease horizon", "The
# lease state file"; D56, D60, D63). Card #0182 (E0.08-T04-S01).
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call and answer from files beside them (conformance/operator/host-commands-common.ps1), over
# a host registry the case writes and a lease state file the case seeds in the home's state folder. Each
# case compares stderr line by line, the exit code, the AWS CLI calls in order, and that the lease state
# file and its folder were left as they were.
#
# Cases:
#   Leases     The count per host's project: pending and unexpired ready leases live, an expired ready,
#              a released and an unsatisfiable lease not, another project's not (the project compared
#              exactly), the box counting swimcoach's, a creating host with no instance ID showing its
#              count after `Instance  none recorded`, ten printed as 10, a byte order mark and
#              whitespace read through, and `-Quiet` withholding none of it.
#   Guard      The line's text and padding; its place after the Leases line and before the next Host
#              line; its time, in the laptop's zone, read here from the same system: from the launch time,
#              from a released lease's released_at, from an expired lease's expires_at, from a lease
#              released after it expired (its expires_at), the later of the two, a lease released early
#              (its released_at, not its expires_at), the latest of several ends, an unsatisfiable lease
#              adding none, rounded up to the next whole minute, and a past-due host showing the report's
#              own minute. No line for a host a live lease explains, a host that is not running, or one
#              with no instance ID.
#   Not read   A missing, unreadable or invalid lease state file: every host's five lines, no Leases or
#              Guard line, the file's message once, after the last host and before any closing count, on
#              stderr whatever -Quiet says, with the exit code the host lines alone give; #0034's message
#              ending the command with that line unprinted; the LaunchTime never read for such a file.
#   Around     LaunchTime missing or malformed failing a running unexplained host's part alone;
#              AGENTSPACE_STATE_DIR moving the file; a run finishing while the case holds leases.lock;
#              `status` and `status -Host` printing their five lines alone over an invalid file.
#
# THE TIME. A Guard line's time is the later of the host's stop time and the report's time, in the
# laptop's zone as the system gives it when the line is printed, rounded up to the next whole minute
# (operator.md, status -All). A case whose stop time lies in the future (launch or lease end five minutes
# before the case's start, so the stop falls ten minutes after it) has one exact answer. A past-due host
# shows the report's own minute, and the report's time is read inside the run, so the case accepts every
# minute from the one after the run's start to the one after its end, and says so in its failure reason.
# The zone is read here with [TimeZoneInfo]::Local after ClearCachedData(), never assumed or stored.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every instance and volume ID and every lease ID
# is made up, and a failure reason never quotes the program's output. Every write and delete goes through
# Assert-ConformancePath at the call.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-status-leases.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$script:Inv = [Globalization.CultureInfo]::InvariantCulture
$script:LeaseSeq = 0
$script:LeaseHold = $null
$script:ActiveDir = $null
$script:T0 = [DateTime]::UtcNow
$script:Snap = $null
$script:CaseSetup = {}

# ---- Time ----
function Initialize-Clock {
  $n = [DateTime]::UtcNow
  $script:T0 = [DateTime]::new($n.Ticks - ($n.Ticks % [TimeSpan]::TicksPerSecond), [DateTimeKind]::Utc)
}
function Get-FloorMinute([DateTime]$T) { [DateTime]::new($T.Ticks - ($T.Ticks % [TimeSpan]::TicksPerMinute), [DateTimeKind]::Utc) }
# The next whole minute, or the time itself when it is one.
function Get-CeilMinute([DateTime]$T) {
  $f = Get-FloorMinute $T
  if ($f.Ticks -eq $T.Ticks) { return $f }
  $f.AddMinutes(1)
}
# `<h:mm><am|pm>` in the laptop's zone, read from the system now (operator.md, status -All).
function Format-LaptopTime([DateTime]$Utc) {
  [TimeZoneInfo]::ClearCachedData()
  $l = [TimeZoneInfo]::ConvertTimeFromUtc($Utc, [TimeZoneInfo]::Local)
  $l.ToString('h:mm', $script:Inv) + $(if ($l.Hour -lt 12) { 'am' } else { 'pm' })
}
function Format-Z([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd\THH:mm:ss\Z", $script:Inv) }
# A launch time as the AWS CLI prints it.
function Format-Launch([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd\THH:mm:ss", $script:Inv) + '+00:00' }

# The Guard line's exact text for a stop time that lies after the report's time.
function Get-GuardFuture([DateTime]$StopUtc) {
  @{ OneOf = @('Guard     unexplained; stops at ' + (Format-LaptopTime (Get-CeilMinute $StopUtc))) }
}
# The Guard lines a past-due host's report MAY show: the minute after the report's time, which was read
# between the run's start and its end (the start's second truncated, since the clock the program reads
# has whole seconds).
function Get-GuardPastDue($Run) {
  $s = $Run.Started
  $first = Get-CeilMinute ([DateTime]::new($s.Ticks - ($s.Ticks % [TimeSpan]::TicksPerSecond), [DateTimeKind]::Utc))
  $last = Get-CeilMinute ($s.AddSeconds([double]$Run.Elapsed + 2))
  $o = [System.Collections.Generic.List[string]]::new()
  for ($t = $first; $t -le $last; $t = $t.AddMinutes(1)) { $o.Add('Guard     unexplained; stops at ' + (Format-LaptopTime $t)) }
  @{ OneOf = $o.ToArray() }
}

# ---- The hosts: a spec per host, the registry and the AWS answers it gives ----
# P project, Id the instance ID or $null, S the instance's state, Reg the record's state, L the launch
# time as the AWS CLI prints it, NoLaunch to leave LaunchTime out of the answer.
function New-HostSpec([string]$P, [string]$Id, [string]$S = 'running', [string]$Reg = 'ready', [string]$L = '2020-01-01T00:00:00+00:00', [switch]$NoLaunch) {
  @{ P = $P; Id = $Id; S = $S; Reg = $Reg; L = $L; NoLaunch = [bool]$NoLaunch }
}
function Set-SpecHosts($Specs) {
  $recs = foreach ($s in $Specs) {
    if ($s.Id) { New-HostRecord -Project $s.P -State $s.Reg -InstanceId $s.Id } else { New-HostRecord -Project $s.P -State $s.Reg -LastAttempt '2026-01-01T00:00:00Z' }
  }
  Set-HostsRecords @($recs)
  $sc = @{}
  foreach ($s in $Specs) {
    if (-not $s.Id) { continue }
    $sc = Merge-Scenario $sc (Get-HealthyAnswers -Id $s.Id -State $s.S)
    $doc = New-InstanceDoc -Id $s.Id -State $s.S -Launch $s.L
    if ($s.NoLaunch) { $doc = $doc -replace ',"LaunchTime":"[^"]*"', '' }
    $sc["ec2 describe-instances $($s.Id)"] = @(New-Step $doc)
  }
  Set-AwsScenario $sc
}
# The AWS calls status -All makes: no call for a host with no instance ID, and no Session Manager call
# for a host that is not running.
function Get-SpecCalls($Specs) {
  foreach ($s in $Specs) {
    if (-not $s.Id) { continue }
    Get-CallDescribe $s.Id
    Get-CallVolumes $s.Id
    if ($s.S -eq 'running') { Get-CallSsm $s.Id }
  }
}
# One host's lines: its Host line, its five lines or `Instance  none recorded`, then the Leases line when
# $Leases is given and the Guard item when $Guard is.
function Get-SpecLines($Spec, $Leases = $null, $Guard = $null) {
  $stack = if ($Spec.P -eq 'swimcoach') { 'agentbox' } else { "agentspace-host-$($Spec.P)" }
  $o = [System.Collections.Generic.List[object]]::new()
  $o.Add("Host      $($Spec.P) ($stack), $($Spec.Reg)")
  if (-not $Spec.Id) { $o.Add('Instance  none recorded') }
  else {
    $ssm = if ($Spec.S -eq 'running') { 'online' } else { '-' }
    foreach ($l in ((Get-StatusLines $Spec.Id $Spec.S 'm9g.xlarge' 20 $ssm) -split "`n")) { if ($l -ne '') { $o.Add($l) } }
  }
  if ($null -ne $Leases) { $o.Add("Leases    $Leases live") }
  if ($null -ne $Guard) { $o.Add($Guard) }
  , $o.ToArray()
}
# One group of lines, kept whole when it is put in a list of groups.
function New-Part { , @($args) }
# Joins the lines of several hosts.
function Join-Report([object[]]$Parts) { $all = [System.Collections.Generic.List[object]]::new(); foreach ($p in $Parts) { foreach ($i in $p) { $all.Add($i) } }; , $all.ToArray() }

# ---- The lease state file ----
function Get-StateDir { $script:Fx.H.StateDir }
# The folder the run reads the file from: the home's state folder, or the one a case names with AGENTSPACE_STATE_DIR.
function Get-ActiveDir { if ($script:ActiveDir) { $script:ActiveDir } else { Get-StateDir } }
function Get-LeaseFilePath([string]$Dir = '') { Join-Path $(if ($Dir) { $Dir } else { Get-StateDir }) 'leases.json' }
function New-Lease {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][ValidateSet('pending', 'ready', 'released', 'unsatisfiable')][string]$State, [string]$Expires = '', [string]$Released = '', [string]$Created = '2026-11-01T00:00:00Z')
  $script:LeaseSeq++
  $n = '{0:x8}' -f $script:LeaseSeq
  $r = [ordered]@{ id = "$n-0000-4000-8000-00000000cafe"; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800; created_at = $Created }
  if ($State -eq 'ready') { $r['expires_at'] = $Expires; $r['ssh'] = [ordered]@{ host_alias = "agentspace-$Project-$n"; user = 'ubuntu' } }
  if ($State -eq 'released') { if ($Expires) { $r['expires_at'] = $Expires }; $r['released_at'] = $Released }
  if ($State -eq 'unsatisfiable') { $r['reasons'] = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
  $r['driver'] = 'ec2'
  $r
}
function Get-LeaseJson($Records) { ([ordered]@{ version = 0; leases = @($Records) } | ConvertTo-Json -Compress -Depth 10) + "`n" }
# Writes the file, after checking the text is valid by the state schema, so that a case's seed is never
# the reason a program refuses it.
function Set-LeaseFile($Records, [switch]$Pretty, [switch]$Bom, [string]$Dir = '') {
  $text = Get-LeaseJson $Records
  if (-not (Test-ConformanceSchema -Json $text -Schema 'state/leases.state.schema.json')) { throw 'The case seeded a lease state file that the state schema refuses.' }
  if ($Pretty) { $text = ([ordered]@{ version = 0; leases = @($Records) } | ConvertTo-Json -Depth 10) + "`r`n" }
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes($text)
  if ($Bom) { $bytes = [byte[]](0xEF, 0xBB, 0xBF) + $bytes }
  $d = if ($Dir) { $Dir } else { Get-StateDir }
  New-HomeFolder $d
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path (Get-LeaseFilePath $d)), $bytes)
}
function Set-LeaseBytes([byte[]]$Bytes) {
  New-HomeFolder (Get-StateDir)
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path (Get-LeaseFilePath)), $Bytes)
}
function Hold-LeaseFile([string]$Name) {
  $script:LeaseHold = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $script:Fx.H -Path (Join-Path (Get-StateDir) $Name)), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
}
function Close-LeaseHold { if ($null -ne $script:LeaseHold) { $script:LeaseHold.Dispose(); $script:LeaseHold = $null } }
# Puts the state folders back to nothing: no lease state file, no lock, no other folder.
function Reset-LeaseState {
  Close-LeaseHold
  Remove-HomePath (Get-StateDir)
  Remove-HomePath (Join-Path $script:Fx.H.Root 'other-state')
  Remove-HomePath (Join-Path $script:Fx.Agentspace 'leases.json')
  New-HomeFolder (Get-StateDir)
  $script:Snap = $null
  $script:ActiveDir = $null
}
function Get-TreeText([string]$Dir) {
  if (-not (Test-Path -LiteralPath $Dir)) { return '<absent>' }
  (@(Get-ChildItem -LiteralPath $Dir -Recurse -Force | Sort-Object FullName | ForEach-Object {
        $rel = $_.FullName.Substring($Dir.Length)
        if ($_.PSIsContainer) { "d $rel" } else { "f $rel $($_.Length) $($_.LastWriteTimeUtc.Ticks)" }
      }) -join "`n")
}
# Remembers the file's bytes and the state folder's listing, after a case's own setup.
function Save-LeaseSnapshot {
  $bytes = $null
  $readable = $true
  try { $bytes = Get-FileBytesOrNull (Get-LeaseFilePath (Get-ActiveDir)) } catch { $readable = $false }
  $script:Snap = @{ Bytes = $bytes; Readable = $readable; Tree = (Get-TreeText (Get-ActiveDir)) }
}
# `status -All` never writes the lease state file and never takes its lock (operator.md, status -All):
# the file's bytes and the folder's listing are as the case left them.
function Test-LeaseStateUntouched {
  $s = $script:Snap
  if ($null -eq $s) { return }
  if ($s.Readable -and -not (Test-SameBytes (Get-FileBytesOrNull (Get-LeaseFilePath (Get-ActiveDir))) $s.Bytes)) { 'The lease state file was changed' }
  if ((Get-TreeText (Get-ActiveDir)) -cne $s.Tree) { 'The lease state folder holds a file that was made, changed or removed (a write, or a take of the lock)' }
}
# D60: counts, never a lease's ID, alias or other member.
function Test-NoLeaseIds([string]$Stderr) {
  if ($Stderr -cmatch '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}') { 'Stderr holds something shaped like a lease ID' }
  if ($Stderr -cmatch 'agentspace-[a-z0-9-]+-[0-9a-f]{8}(\b|$)') { 'Stderr holds something shaped like a lease alias' }
}

# ---- The report ----
# Compares stderr line by line with what it MUST be. An item is a line's exact text, @{OneOf = <lines>} for
# a line with several permitted texts, or @{Pattern = <regular expression>}.
function Compare-Report([string]$Stderr, [object[]]$Want) {
  $got = @($Stderr -split "`n")
  if ($got.Count -gt 0 -and $got[-1] -eq '') { $got = @($got | Select-Object -SkipLast 1) }
  for ($i = 0; $i -lt [Math]::Max($got.Count, $Want.Count); $i++) {
    if ($i -ge $Want.Count) { return "Stderr has more lines than the $($Want.Count) the spec gives" }
    $w = $Want[$i]
    if ($i -ge $got.Count) { return "Stderr stops before line $($i + 1), which is $(if ($w -is [string]) { "'$w'" } elseif ($w.OneOf) { "'$($w.OneOf[0])'" } else { 'the lease state file''s message' })" }
    $line = $got[$i]
    if ($line.Contains("`r")) { return "Stderr line $($i + 1) holds a carriage return" }
    if ($w -is [string]) {
      if ($line -cne $w) { return "Stderr line $($i + 1) is not '$w'" }
    } elseif ($w.ContainsKey('OneOf')) {
      if ($line -cnotin $w.OneOf) {
        $shape = if ($w.OneOf.Count -eq 1) { "'$($w.OneOf[0])'" } else { "one of $($w.OneOf.Count) lines from '$($w.OneOf[0])' to '$($w.OneOf[-1])'" }
        return "Stderr line $($i + 1), a Guard line, is not $shape"
      }
    } else {
      if ($line -cnotmatch $w.Pattern) { return "Stderr line $($i + 1) is not the lease state file's message in operator.md's form" }
    }
  }
}
function Test-StatusReport($Run, [object[]]$Want) {
  Compare-Report $Run.Stderr $Want
  Test-LeaseStateUntouched
  Test-NoLeaseIds $Run.Stderr
}
# The lease state file's message: operator.md, status -All (P24). The reason is one fragment with no line
# break, the system's own error text or a first problem; `the file does not exist` is pinned for a missing file.
function Get-LeaseMessage([string]$Dir = '', [switch]$Missing) {
  $path = Get-LeaseFilePath $Dir
  $long = $path
  try { $long = Assert-ConformancePath -ConformanceHome $script:Fx.H -Path $path } catch { }
  $paths = @($path, $long | Select-Object -Unique | ForEach-Object { [regex]::Escape($_) }) -join '|'
  if ($Missing) { return @{ Pattern = '^agentspace: The lease state file (' + $paths + ') cannot be read: the file does not exist\.$' } }
  @{ Pattern = '^agentspace: The lease state file (' + $paths + ') cannot be read: [^\r\n]*[^\s.\r\n]\.$' }
}
function Get-CountLines($Run) {
  $all = @($Run.Stderr -split "`n" | Where-Object { $_ -ne '' })
  @($all | Where-Object { $_ -cmatch '^agentspace: The lease state file ' }).Count
}

# Runs one case: a wrapper over Test-HostCase that first puts the lease state back to nothing and takes
# the snapshot after the case's own setup.
function Test-StatusCase {
  param([Parameter(Mandatory)][string]$CaseName, [Parameter(Mandatory)][string[]]$CaseArguments, [Parameter(Mandatory)][hashtable]$CaseExpect, [Parameter(Mandatory)][scriptblock]$Prepare, [hashtable]$CaseEnvironment = @{})
  $script:CaseSetup = $Prepare
  Test-HostCase -Name $CaseName -Arguments $CaseArguments -Expect $CaseExpect -Environment $CaseEnvironment -Setup {
    Initialize-Clock
    Reset-LeaseState
    & $script:CaseSetup
    Save-LeaseSnapshot
  }
}

$signIn = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry."
$denied = 'An error occurred (UnauthorizedOperation) when calling the DescribeVolumes operation: You are not authorized to perform this operation.'

Invoke-HostFile -FileName 'test-unmet-status-leases.ps1' -Body {
  param($fx)
  try {
    $idBox = $script:IdBox
    $i1 = 'i-5a5a5a5a5a5a5a5a5'; $i2 = 'i-6b6b6b6b6b6b6b6b6'; $i3 = 'i-7c7c7c7c7c7c7c7c7'; $i4 = 'i-8d8d8d8d8d8d8d8d8'
    $i5 = 'i-9e9e9e9e9e9e9e9e9'; $i6 = 'i-1f1f1f1f1f1f1f1f1'; $i7 = 'i-2a2a2a2a2a2a2a2a2'
    $secS = 'operator.md#status--all'
    $secE = 'operator.md#every-host-at-once--all'
    $secP = 'operator.md#status'

    # ============ The Leases line ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'running'), (New-HostSpec 'agentspace' $i1 'stopped'), (New-HostSpec 'gamma' $null 'running' 'creating'), (New-HostSpec 'zeta' $i3 'running'))
    Test-StatusCase "${secS}: Leases <n> live follows each host's lines: pending and unexpired ready leases are live; an expired ready, a released and an unsatisfiable lease, and another project's leases (compared exactly), are not; the box counts swimcoach's; a creating host with no instance ID shows its count after Instance  none recorded; 10 prints as 10, and a host a live lease explains has no Guard line" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 1), (Get-SpecLines $sp[1] 0), (Get-SpecLines $sp[2] 1), (Get-SpecLines $sp[3] 10)))
      } } -Prepare {
      Set-SpecHosts $sp
      $t = $script:T0
      $zl = @()
      $zl += New-Lease 'swimcoach' 'ready' -Expires (Format-Z $t.AddHours(2))
      $zl += New-Lease 'swimcoach' 'ready' -Expires (Format-Z $t.AddHours(-1))
      $zl += New-Lease 'swimcoach' 'released' -Expires (Format-Z $t.AddHours(1)) -Released (Format-Z $t.AddMinutes(-30))
      $zl += New-Lease 'agentbox' 'ready' -Expires (Format-Z $t.AddHours(2))
      $zl += New-Lease 'agentspace' 'unsatisfiable'
      $zl += New-Lease 'agentspace' 'released' -Released (Format-Z $t.AddMinutes(-20))
      $zl += New-Lease 'agentspace' 'ready' -Expires (Format-Z $t.AddMinutes(-40))
      $zl += New-Lease 'agent' 'ready' -Expires (Format-Z $t.AddHours(2))
      $zl += New-Lease 'agentspace-2' 'pending'
      $zl += New-Lease 'gamma' 'pending'
      $zl += New-Lease 'gamma' 'unsatisfiable'
      for ($k = 1; $k -le 6; $k++) { $zl += New-Lease 'zeta' 'ready' -Expires (Format-Z $t.AddMinutes(30 + $k)) }
      for ($k = 1; $k -le 4; $k++) { $zl += New-Lease 'zeta' 'pending' }
      Set-LeaseFile $zl
    }

    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped'), (New-HostSpec 'agentspace' $i1 'running'))
    Test-StatusCase "${secS}: a file with a byte order mark and whitespace is read as any other: the Leases count is 1, and a running host a live ready lease explains has no Guard line" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started) Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 1))) } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'ready' -Expires (Format-Z $script:T0.AddHours(1)))) -Pretty -Bom
    }
    Test-StatusCase "${secS}: a running host whose project holds only a pending lease is explained by it: Leases 1 live, and no Guard line" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started) Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 1))) } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'))
    }
    Test-StatusCase "${secS}: a ready lease that expired six hours ago does not explain its host, which is unexplained: Leases 0 live and a Guard line, its stop the launch time plus 15 minutes" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started) Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))))) } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'ready' -Expires (Format-Z $script:T0.AddHours(-6))))
    }

    # ============ The Guard line ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped'), (New-HostSpec 'agentspace' $i1 'running'), (New-HostSpec 'zeta' $i2 'stopped'))
    Test-StatusCase "${secS}: a running host no live lease explains gets Guard     unexplained; stops at <h:mm><am|pm> after its Leases 0 live line and before the next host's Host line, the time its launch time plus 15 minutes, in the laptop's zone" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @()
    }
    Test-StatusCase "${secS}: the same report with -Quiet: every Host line, five lines, Leases line and Guard line is still printed" @('status', '-All', '-Quiet') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @()
    }
    Test-StatusCase "${secS}: the stop time is the later of the launch time and the last lease end: a lease that ended hours before the launch adds nothing" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'released' -Released (Format-Z $script:T0.AddHours(-3))))
    }
    Test-StatusCase "${secS}: a released lease's released_at, when it is the later of the two, sets the stop time: a lease released five minutes ago, never ready, gives a stop ten minutes from now" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddHours(-3)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'released' -Released (Format-Z $script:T0.AddMinutes(-5))))
    }
    Test-StatusCase "${secS}: a lease released before it expired ends at its released_at, never at the later expires_at (state.md, P11): released five minutes ago, expiring in two hours, gives a stop ten minutes from now" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddHours(-3)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'released' -Expires (Format-Z $script:T0.AddHours(2)) -Released (Format-Z $script:T0.AddMinutes(-5))))
    }
    Test-StatusCase "${secS}: an expired lease's expires_at, when it is the later of the two, sets the stop time: a ready lease that expired five minutes ago gives a stop ten minutes from now" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddHours(-3)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'ready' -Expires (Format-Z $script:T0.AddMinutes(-5))))
    }
    Test-StatusCase "${secS}: a lease released after it expired ends at its expires_at (state.md, P11): expired five minutes ago and released one minute ago gives a stop ten minutes from now, not fourteen" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddHours(-3)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'released' -Expires (Format-Z $script:T0.AddMinutes(-5)) -Released (Format-Z $script:T0.AddMinutes(-1))))
    }
    Test-StatusCase "${secS}: the project's last lease end is the latest end among its leases: ends 12, 9 and 5 minutes ago give a stop ten minutes from now" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddHours(-3)
      Set-SpecHosts $sp
      Set-LeaseFile @(
        (New-Lease 'agentspace' 'released' -Released (Format-Z $script:T0.AddMinutes(-12))),
        (New-Lease 'agentspace' 'released' -Released (Format-Z $script:T0.AddMinutes(-9))),
        (New-Lease 'agentspace' 'ready' -Expires (Format-Z $script:T0.AddMinutes(-5))),
        (New-Lease 'swimcoach' 'released' -Released (Format-Z $script:T0.AddMinutes(-1))))
    }
    Test-StatusCase "${secS}: an unsatisfiable lease adds no lease end: with a launch five minutes ago and an unsatisfiable lease created one minute ago, the stop is ten minutes from now" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $script:T0.AddMinutes(10))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'unsatisfiable' -Created (Format-Z $script:T0.AddMinutes(-1))))
    }
    Test-StatusCase "${secS}: a stop time on a whole minute is printed as it is, not rounded up" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture ((Get-FloorMinute $script:T0).AddMinutes(10)))), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch ((Get-FloorMinute $script:T0).AddMinutes(-5))
      Set-SpecHosts $sp
      Set-LeaseFile @()
    }
    Test-StatusCase "${secS}: a stop time with seconds is rounded up to the next whole minute: a stop time 7 seconds past a whole minute is printed as the next whole minute" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        $stop = (Get-FloorMinute $script:T0).AddMinutes(10).AddSeconds(7)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardFuture $stop)), (Get-SpecLines $sp[2] 0)))
      } } -Prepare {
      $sp[1].L = Format-Launch ((Get-FloorMinute $script:T0).AddMinutes(-5).AddSeconds(7))
      Set-SpecHosts $sp
      Set-LeaseFile @()
    }

    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped'), (New-HostSpec 'agentspace' $i1 'running'), (New-HostSpec 'zeta' $i2 'running'))
    Test-StatusCase "${secS}: a past-due host shows the report's own minute, never a time already past: launched in 2020 and its last lease ended long ago, the line names the minute after the run's start or the one after its end; a host whose stop is still ahead shows its stop, and each Guard line follows its own host" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 0 (Get-GuardPastDue $run)), (Get-SpecLines $sp[2] 0 (Get-GuardFuture $script:T0.AddMinutes(10)))))
      } } -Prepare {
      $sp[2].L = Format-Launch $script:T0.AddMinutes(-5)
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'released' -Released '2026-11-01T00:00:00Z'))
    }

    # ============ No Guard line ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped'), (New-HostSpec 'agentspace' $i1 'stopping'), (New-HostSpec 'zeta' $i2 'pending'), (New-HostSpec 'eta' $i3 'shutting-down'), (New-HostSpec 'theta' $i4 'terminated'), (New-HostSpec 'gamma' $null 'running' 'creating'), (New-HostSpec 'delta' $null 'running' 'failed'))
    Test-StatusCase "${secS}: a stopped, stopping, pending, shutting-down or terminated host, and a creating or failed host with no instance ID, each shows Leases 0 live and no Guard line, though no live lease explains it" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @($sp | ForEach-Object { , (Get-SpecLines $_ 0) }))
      } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseFile @()
    }

    # ============ A lease state file that is not read ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'running'), (New-HostSpec 'agentspace' $i1 'running'), (New-HostSpec 'gamma' $null 'running' 'creating'))
    $kinds = [ordered]@{
      'that is missing'                            = { }
      'that is not JSON'                           = { Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('this is not json' + "`n")) }
      'that is empty'                              = { Set-LeaseBytes ([byte[]]@()) }
      'whose bytes are not UTF-8'                  = { Set-LeaseBytes ([byte[]](0x7B, 0x22, 0xFF, 0xFE, 0x22, 0x7D)) }
      'whose version is not 0'                    = { Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('{"version":1,"leases":[]}' + "`n")) }
      'that holds two records with one id'        = { $r = New-Lease 'agentspace' 'pending'; $t = ([ordered]@{ version = 0; leases = @($r, $r) } | ConvertTo-Json -Compress -Depth 10); Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes($t + "`n")) }
      'that holds a record naming a member twice' = { $t = (Get-LeaseJson @((New-Lease 'agentspace' 'pending'))) -replace '"size":"m"', '"size":"m","size":"m"'; Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes($t)) }
      'that holds a time that names no real instant' = { $t = (Get-LeaseJson @((New-Lease 'agentspace' 'ready' -Expires '2026-12-01T00:00:00Z'))) -replace '2026-12-01T00:00:00Z', '2026-02-30T00:00:00Z'; Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes($t)) }
      'that is a folder'                          = { New-HomeFolder (Get-LeaseFilePath) }
      'that cannot be opened'                     = { Set-LeaseFile @((New-Lease 'agentspace' 'pending')); Hold-LeaseFile 'leases.json' }
    }
    foreach ($kind in $kinds.Keys) {
      $script:KindSetup = $kinds[$kind]
      $isMissing = ($kind -eq 'that is missing')
      Test-StatusCase "${secS}: a lease state file $kind leaves out every Leases and Guard line, prints each host's five lines and the lease state file's message once after the last host, and exits 0" @('status', '-All') @{
        Exit = 0; Calls = @(Get-SpecCalls $sp)
        Check = { param($run, $started)
          $msg = if ($isMissing) { Get-LeaseMessage -Missing } else { Get-LeaseMessage }
          $want = Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (Get-SpecLines $sp[2]), (New-Part $msg))
          Test-StatusReport $run $want
          Close-LeaseHold
        } } -Prepare {
        Set-SpecHosts $sp
        & $script:KindSetup
      }
    }
    Test-StatusCase "${secS}: a missing state folder is a missing file: the message is the same, and the run makes no folder" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (Get-SpecLines $sp[2]), (New-Part (Get-LeaseMessage -Missing))))
        if (Test-Path -LiteralPath (Get-StateDir)) { 'The run made the state folder' }
      } } -Prepare {
      Set-SpecHosts $sp
      Remove-HomePath (Get-StateDir)
    }
    Test-StatusCase "${secS}: the lease state file's message is on stderr whatever -Quiet says, with every host's report: a missing file with -Quiet" @('status', '-All', '-Quiet') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (Get-SpecLines $sp[2]), (New-Part (Get-LeaseMessage -Missing))))
      } } -Prepare { Set-SpecHosts $sp }
    Test-StatusCase "${secS}: an invalid file with -Quiet prints the message too" @('status', '-All', '-Quiet') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (Get-SpecLines $sp[2]), (New-Part (Get-LeaseMessage))))
      } } -Prepare { Set-SpecHosts $sp; Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('not json')) }
    Test-StatusCase "${secS}: a missing file is not no leases: a running host the leases would leave unexplained and past its stop time shows no Guard line, and its LaunchTime is not read (an answer with none still reports it)" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (Get-SpecLines $sp[2]), (New-Part (Get-LeaseMessage -Missing))))
      } } -Prepare {
      $sp[1].NoLaunch = $true
      Set-SpecHosts $sp
      $sp[1].NoLaunch = $false
    }

    # The exit code is the one the host lines alone give, and the message comes before the closing count.
    $sp = @((New-HostSpec 'swimcoach' $idBox 'running'), (New-HostSpec 'agentspace' $i1 'running'), (New-HostSpec 'gamma' $null 'running' 'creating'))
    foreach ($kind in 'missing', 'invalid') {
      $script:KindSetup = if ($kind -eq 'missing') { { } } else { { Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('not json')) } }
      $isMissing = ($kind -eq 'missing')
      Test-StatusCase "${secE}: $(if ($kind -eq 'missing') { 'a missing' } else { 'an invalid' }) lease state file and a host whose call failed: the failed host has its Host line and failure line alone, the lease state file's message comes after the last host and before agentspace: 1 of 3 hosts failed., exit 1, and the message is not counted as a failure" @('status', '-All') @{
        Exit = 1
        Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox), (Get-CallDescribe $i1), (Get-CallVolumes $i1))
        Check = { param($run, $started)
          $msg = if ($isMissing) { Get-LeaseMessage -Missing } else { Get-LeaseMessage }
          $want = Join-Report @((Get-SpecLines $sp[0]), (New-Part 'Host      agentspace (agentspace-host-agentspace), ready' "agentspace: $denied"), (Get-SpecLines $sp[2]), (New-Part $msg 'agentspace: 1 of 3 hosts failed.'))
          Test-StatusReport $run $want
        } } -Prepare {
        Set-SpecHosts $sp
        Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $idBox) (Get-HealthyAnswers -Id $i1) @{ "ec2 describe-volumes $(Get-VolumeOf $i1)" = @(New-Step '' $denied 254) })
        & $script:KindSetup
      }
    }
    Test-StatusCase "${secE}: #0034's sign-in message ends status -All at once with the lease state file's line unprinted: a missing file, and the sign-in failing the second host's first call" @('status', '-All') @{
      Exit = 1
      Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox), (Get-CallDescribe $i1))
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (New-Part 'Host      agentspace (agentspace-host-agentspace), ready' $signIn)))
      } } -Prepare {
      Set-SpecHosts $sp
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $idBox) @{ "ec2 describe-instances $i1" = @(New-Step '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255) })
    }
    Test-StatusCase "${secE}: #0034's sign-in message ends status -All at once over a valid file too: the first host's lines, Leases line included, are printed, and nothing follows the message" @('status', '-All') @{
      Exit = 1
      Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox), (Get-CallDescribe $i1))
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 1), (New-Part 'Host      agentspace (agentspace-host-agentspace), ready' $signIn)))
      } } -Prepare {
      Set-SpecHosts $sp
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $idBox) @{ "ec2 describe-instances $i1" = @(New-Step '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255) })
      Set-LeaseFile @((New-Lease 'swimcoach' 'ready' -Expires (Format-Z $script:T0.AddHours(1))))
    }

    # ============ A failed host with a valid file; LaunchTime ============
    Test-StatusCase "${secE}: with a valid file, a host whose call failed has its Host line and its failure line, no Leases or Guard line, and the report goes on: 1 of 3 hosts failed, exit 1" @('status', '-All') @{
      Exit = 1
      Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox), (Get-CallDescribe $i1), (Get-CallVolumes $i1))
      Check = { param($run, $started)
        $want = Join-Report @((Get-SpecLines $sp[0] 1), (New-Part 'Host      agentspace (agentspace-host-agentspace), ready' "agentspace: $denied"), (Get-SpecLines $sp[2] 0), (New-Part 'agentspace: 1 of 3 hosts failed.'))
        Test-StatusReport $run $want
      } } -Prepare {
      Set-SpecHosts $sp
      Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $idBox) (Get-HealthyAnswers -Id $i1) @{ "ec2 describe-volumes $(Get-VolumeOf $i1)" = @(New-Step '' $denied 254) })
      Set-LeaseFile @((New-Lease 'swimcoach' 'ready' -Expires (Format-Z $script:T0.AddHours(1))))
    }

    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped' -NoLaunch), (New-HostSpec 'agentspace' $i1 'running' -NoLaunch), (New-HostSpec 'zeta' $i2 'running' 'ready' '2020-01-01T00:00:00'), (New-HostSpec 'eta' $i3 'running' -NoLaunch), (New-HostSpec 'theta' $i4 'stopped' -NoLaunch))
    $launchMsg = 'agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.'
    Test-StatusCase "${secS}: LaunchTime is read only for a host that gets a Guard line: a running unexplained host with none, or one with no UTC offset, fails its part alone before its five lines (Host line, then the message), and a stopped host or a running host a live lease explains, with none, is reported as usual: 2 of 5 hosts failed, exit 1" @('status', '-All') @{
      Exit = 1
      Check = { param($run, $started)
        $want = Join-Report @((Get-SpecLines $sp[0] 0), (New-Part 'Host      agentspace (agentspace-host-agentspace), ready' $launchMsg), (New-Part 'Host      zeta (agentspace-host-zeta), ready' $launchMsg), (Get-SpecLines $sp[3] 1), (Get-SpecLines $sp[4] 0), (New-Part 'agentspace: 2 of 5 hosts failed.'))
        Test-StatusReport $run $want
      } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'eta' 'ready' -Expires (Format-Z $script:T0.AddHours(1))))
    }

    # ============ Around the file ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'stopped'), (New-HostSpec 'agentspace' $i1 'running'))
    Test-StatusCase "${secS}: a run finishes while the case holds the lease state lock: leases.lock is never taken, so the report is the usual one within the seconds its calls take, and the lock file is as the case made it" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 1)))
        if ($run.Elapsed -ge 20) { "It took $([int]$run.Elapsed) seconds, so it waited for the lock" }
        Close-LeaseHold
      } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'))
      Hold-LeaseFile 'leases.lock'
    }
    $other = Join-Path $fx.H.Root 'other-state'
    Test-StatusCase "${secS}: AGENTSPACE_STATE_DIR moves the lease state file: the counts are those of the file in that folder, not of one in the default state folder or beside hosts.json" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0] 0), (Get-SpecLines $sp[1] 2)))
      } } -CaseEnvironment @{ AGENTSPACE_STATE_DIR = $other } -Prepare {
      Set-SpecHosts $sp
      $script:ActiveDir = $other
      $t = $script:T0
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'ready' -Expires (Format-Z $t.AddHours(1)))) -Dir $other
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'))
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending'), (New-Lease 'agentspace' 'pending')) -Dir $fx.Agentspace
    }
    Test-StatusCase "${secS}: AGENTSPACE_STATE_DIR moving the file to a folder with none: the message names that folder's file and says the file does not exist, whatever the default state folder holds" @('status', '-All') @{
      Exit = 0; Calls = @(Get-SpecCalls $sp)
      Check = { param($run, $started)
        Test-StatusReport $run (Join-Report @((Get-SpecLines $sp[0]), (Get-SpecLines $sp[1]), (New-Part (Get-LeaseMessage -Dir $other -Missing))))
      } } -CaseEnvironment @{ AGENTSPACE_STATE_DIR = $other } -Prepare {
      Set-SpecHosts $sp
      New-HomeFolder $other
      $script:ActiveDir = $other
      Set-LeaseFile @((New-Lease 'agentspace' 'pending'))
    }

    # ============ status and status -Host never read the file ============
    $sp = @((New-HostSpec 'swimcoach' $idBox 'running'), (New-HostSpec 'agentspace' $i1 'running'))
    Test-StatusCase "${secP}: status prints the five lines alone over a lease state file that is not valid: no lease line, no message about the file, exit 0" @('status') @{
      Exit = 0; Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox))
      Check = { param($run, $started) Compare-Report $run.Stderr @(@((Get-StatusLines $idBox).TrimEnd("`n") -split "`n")) } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('not json'))
    }
    Test-StatusCase "${secP}: status prints the five lines alone with a folder at the lease state file's path" @('status') @{
      Exit = 0; Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallSsm $idBox))
      Check = { param($run, $started) Compare-Report $run.Stderr @(@((Get-StatusLines $idBox).TrimEnd("`n") -split "`n")) } } -Prepare {
      Set-SpecHosts $sp
      New-HomeFolder (Get-LeaseFilePath)
    }
    Test-StatusCase "${secP}: status -Host <project> prints that host's five lines alone over a lease state file that is not valid, then signs: exit 0, no lease line, no message about the file" @('status', '-Host', 'agentspace') @{
      Exit = 0; Calls = @((Get-CallDescribe $i1), (Get-CallVolumes $i1), (Get-CallSsm $i1))
      Check = { param($run, $started)
        Compare-Report $run.Stderr @(@((Get-StatusLines $i1).TrimEnd("`n") -split "`n"))
        Get-SignProblems 'agentspace'
      } } -Prepare {
      Set-SpecHosts $sp
      Set-LeaseBytes ([Text.Encoding]::UTF8.GetBytes('not json'))
    }
  } finally {
    Close-LeaseHold
  }
}
Exit-ConformanceRun

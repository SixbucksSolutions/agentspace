#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which know no host registry and no host
# aliases (the suite's defaults until E0.07-T05-S01 installs v0.07). A v0.07 build MUST pass it. It is
# not a regression when it fails against v0.06 (docs/conformance.md, "The files that are expected to
# fail").
#
# A conformance check (D34, D53; card #0155, E0.07-T04-S01): the alias file a ready `create` writes,
# byte for byte, for the box and for a host that is not the box, and a `create` for a project whose
# host is ready while another lease of it is pending. It enforces:
#   - docs/driver.md, "Conformance cases for v0.07", cases C3, C4 and C20, and the sections they cite:
#     "Where it places a lease" and "Starting a provisioning run" (from place).
#   - docs/state.md, "The alias file" (the box's bytes, which are v0.06's), "A lease alias for a host
#     that is not the box" (HostKeyAlias agentspace-host.<project>, then UserKnownHostsFile with
#     agentspace's known_hosts by its full path), "The host registry" and "Agentspace's host keys".
#
# - C3   A swimcoach lease with no hosts.json has v0.06's alias bytes, starts nothing and leaves
#        hosts.json absent. A second case keeps the same bytes when the registry holds another ready
#        host: the record the lease is placed on decides the form (state.md, "A lease alias for a host
#        that is not the box").
# - C4   A lease of a project whose record is ready and is not the box has the alias of state.md, "A
#        lease alias for a host that is not the box", byte for byte, and starts nothing. The record
#        carries no instance_id, which `create` MUST NOT need (D21). A second case writes the project
#        with capitals, which is normalized before the lookup, and a hyphen.
# - C20  A `create` for a ready host's project, while another lease of the project is pending, answers
#        ready and starts nothing, whatever the record's last_attempt_at, and changes neither the
#        registry nor the pending lease.
#
# - Each case writes its own hosts.json and, where it needs one, its own pending record in the lease
#   state file, in the throwaway home, in the forms of docs/state.md, and each of those writes calls
#   Assert-ConformancePath at the call. It depends on no new harness helper.
# - Every `create` is given AGENTSPACE_PROVISION_CMD, a .cmd stand-in under the home that records,
#   beside itself, its arguments and its start time, and nothing of its environment. It makes no host.
#   "Started nothing" is shown by no record after a wait of 4 seconds. A detached start (WMI's
#   Win32_Process.Create) returns the process ID before the stand-in's interpreter has run, and the
#   stand-in's first line takes tens of milliseconds, so 4 seconds is a hundred times that and still
#   a small part of the run. A start that the case expects is waited for up to 30 seconds.
# - The program under test is untrusted (docs/conformance.md): a lease ID or an alias from its output
#   names a file only after its document passed the schema and the patterns (New-CcReadyLease, then
#   Get-CcAliasPath and Get-CcLeaseDir). No failure reason quotes the program's output but a lease
#   document, which holds no secret.
# - It drives the program AGENTSPACE_CMD names only through the harness, as a separate process in a
#   throwaway home with a throwaway CA. It reads no real folder and loads no prototype code.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-host-alias-bytes.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$utf8 = [Text.UTF8Encoding]::new($false)
$script:PgNegWaitMs = 4000
$script:PgPosWaitMs = 30000
$script:PgStandInCount = 0
$wherePlaces = 'driver.md#where-it-places-a-lease'
$aliasOther = 'state.md#a-lease-alias-for-a-host-that-is-not-the-box'
$aliasBox = 'state.md#the-alias-file'

# --- Files the cases seed, each written through Assert-ConformancePath at the call ---------------

function Write-PgFile {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Text)
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $Path), $utf8.GetBytes($Text))
}

# One record of the host registry (state.md, "The host registry"), members in the order the table gives.
function New-PgHostRecord {
  param([Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][string]$State, [string]$LastAttemptAt = '', [switch]$NoInstanceType)
  $r = [ordered]@{
    project = $Project
    stack = $(if ($Project -ceq 'swimcoach') { 'agentbox' } else { "agentspace-host-$Project" })
    state = $State
  }
  # A ready record carries instance_type m9g.xlarge, between instance_id and last_attempt_at, unless -NoInstanceType.
  if ($State -ceq 'ready' -and -not $NoInstanceType) { $r['instance_type'] = 'm9g.xlarge' }
  if ($LastAttemptAt) { $r['last_attempt_at'] = $LastAttemptAt }
  return $r
}

# Writes hosts.json as state.md says: compact JSON, UTF-8 with no byte order mark, one LF.
function Write-PgHosts {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  $text = '{"version":0,"hosts":[' + ((@($Records) | ForEach-Object { ConvertTo-Json -InputObject $_ -Compress }) -join ',') + ']}' + "`n"
  Write-PgFile -Lab $Lab -Path (Join-Path $Lab.UserProfile '.agentspace\hosts.json') -Text $text
}

function Get-PgHostsHash { param($Lab) Get-CcFileHash (Join-Path $Lab.UserProfile '.agentspace\hosts.json') }

# Writes a pending lease record into the lease state file (state.md, "The lease state file"): the
# common fields and driver, and none of expires_at, released_at, ssh or reasons.
function New-PgPending {
  param([Parameter(Mandatory)][string]$Project)
  [ordered]@{
    id = [guid]::NewGuid().ToString()
    project = $Project
    state = 'pending'
    platform = 'linux/arm64'
    profile = 'agent-linux'
    size = 'm'
    ttl_seconds = 28800
    created_at = (ConvertTo-CcTime (Get-CcNowSecond).AddMinutes(-2))
    driver = 'ec2'
  }
}
function Write-PgLeases {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Records)
  Write-CcState $Lab ([ordered]@{ version = 0; leases = @($Records) })
}

# Puts the throwaway home back to nothing seeded: no registry, no project list, no lease records, no
# lease folders, no host folders and an empty ssh folder. Each delete is checked at the call.
function Reset-PgCase {
  param([Parameter(Mandatory)]$Lab)
  foreach ($p in @(
      (Join-Path $Lab.UserProfile '.agentspace\hosts.json'), (Join-Path $Lab.UserProfile '.agentspace\projects.json'),
      (Join-Path $Lab.UserProfile '.agentspace\hosts'), (Join-Path $Lab.UserProfile '.agentspace\leases'),
      (Join-Path $Lab.StateDir 'leases.json'))) {
    if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $p) -Recurse -Force }
  }
  Reset-CcSshDir $Lab
}

# --- The stand-in for the provisioning run (driver.md, "Conformance cases for v0.07") ----------------

# A .cmd under the home that appends its arguments, and its start time, beside itself, and exits 0. It
# makes no host and records nothing of its environment. Each case gets a folder of its own, so a late
# start of one case never counts in the next.
function New-PgStandIn {
  param([Parameter(Mandatory)]$Lab)
  $script:PgStandInCount++
  $dir = Join-Path $Lab.Root "standins\run$($script:PgStandInCount)"
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $dir) | Out-Null
  $cmd = Join-Path $dir 'provision.cmd'
  $text = "@echo off`r`n>>`"%~dp0record.txt`" echo %*`r`n>>`"%~dp0start-time.txt`" echo %DATE% %TIME%`r`nexit /b 0`r`n"
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $cmd), [Text.Encoding]::ASCII.GetBytes($text))
  [pscustomobject]@{ Cmd = $cmd; Record = (Join-Path $dir 'record.txt') }
}

# The lines the stand-in has recorded so far, one per start. A read, sharing with a writer.
function Get-PgStarts {
  param([Parameter(Mandatory)]$StandIn)
  if (-not (Test-Path -LiteralPath $StandIn.Record -PathType Leaf)) { return , @() }
  try {
    $fs = [IO.File]::Open($StandIn.Record, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try { $t = [IO.StreamReader]::new($fs).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return , @() }
  return , @(($t -split "\r?\n") | Where-Object { $_ -ne '' })
}

# Waits up to $script:PgPosWaitMs for one start, then a little longer for a second, and returns the lines.
function Wait-PgStart {
  param([Parameter(Mandatory)]$StandIn)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ((Get-PgStarts $StandIn).Count -lt 1 -and $sw.ElapsedMilliseconds -lt $script:PgPosWaitMs) { Start-Sleep -Milliseconds 100 }
  Start-Sleep -Milliseconds 1500
  return , (Get-PgStarts $StandIn)
}

# Waits the fixed time that shows "started nothing", and returns the lines.
function Wait-PgNoStart {
  param([Parameter(Mandatory)]$StandIn)
  Start-Sleep -Milliseconds $script:PgNegWaitMs
  return , (Get-PgStarts $StandIn)
}

# --- What an alias file MUST hold ------------------------------------------------------------------------

# The lines of an alias file after its owner mark, exactly as state.md gives them, with the trailing LF.
# $IsBox chooses v0.06's block (HostKeyAlias agentspace) or the other host's (HostKeyAlias
# agentspace-host.<project>, then UserKnownHostsFile). Every path is built as the program is told to
# write it: the home as text, then `\` as `/` and `%` as `%%`.
function Get-PgExpectedAliasRest {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$Alias, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][bool]$IsBox)
  $leaseDir = Get-CcLeaseDir $Lab $Id
  $key = (Join-Path $leaseDir 'id_ed25519').Replace('\', '/').Replace('%', '%%')
  $cert = (Join-Path $leaseDir 'id_ed25519-cert.pub').Replace('\', '/').Replace('%', '%%')
  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.AddRange([string[]]@("Host $Alias", '  User ubuntu', "  IdentityFile `"$key`"", "  CertificateFile `"$cert`"", '  IdentitiesOnly yes'))
  if ($IsBox) { $lines.Add('  HostKeyAlias agentspace') }
  else {
    $known = (Join-Path $Lab.UserProfile '.agentspace\known_hosts').Replace('\', '/').Replace('%', '%%')
    $lines.Add("  HostKeyAlias agentspace-host.$Project")
    $lines.Add("  UserKnownHostsFile `"$known`"")
  }
  $lines.AddRange([string[]]@('  StrictHostKeyChecking yes', "  ProxyCommand $($script:CcProxy.Replace('%', '%%'))", '  ServerAliveInterval 30', '  ServerAliveCountMax 4',
      '  RemoteForward 127.0.0.1:8811 127.0.0.1:8811', '  RemoteForward 127.0.0.1:53147 127.0.0.1:53147'))
  return (($lines -join "`n") + "`n")
}

# The problems with an alias file's bytes: UTF-8 with no byte order mark and no CR, a first line that is
# the owner mark naming the lease, and every other byte the expected block's. Never quotes the file.
function Get-PgAliasProblems {
  param([Parameter(Mandatory)][byte[]]$Bytes, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$ExpectedRest)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF) { $p.Add('It starts with a byte order mark') }
  if ($Bytes -contains 13) { $p.Add('It holds a CR') }
  $nl = [Array]::IndexOf($Bytes, [byte]10)
  if ($nl -lt 0) { $p.Add('It holds no LF'); return , $p }
  $first = $utf8.GetString($Bytes, 0, $nl)
  if ($first -cnotmatch "\A# agentspace lease $([regex]::Escape($Id)); [^\r\n]+\z") { $p.Add('The first line is not the owner mark naming the lease, a semicolon and a comment') }
  $rest = [byte[]]$Bytes[($nl + 1)..($Bytes.Length - 1)]
  $want = $utf8.GetBytes($ExpectedRest)
  if (-not [Linq.Enumerable]::SequenceEqual($rest, $want)) {
    $got = $utf8.GetString($rest)
    $gl = @($got -split "`n"); $wl = @($ExpectedRest -split "`n")
    $at = 0; while ($at -lt $gl.Count -and $at -lt $wl.Count -and $gl[$at] -ceq $wl[$at]) { $at++ }
    $p.Add("The lines after the owner mark differ from the block state.md gives, first at its line $($at + 2): wanted '$(if ($at -lt $wl.Count) { $wl[$at] })', found '$(if ($at -lt $gl.Count) { $gl[$at] })'")
  }
  return , $p
}

function Get-PgAliasBytes {
  param($Lab, $Doc)
  $path = Get-CcAliasPath $Lab ([string]$Doc['ssh']['host_alias'])
  if (Test-Path -LiteralPath $path -PathType Leaf) { return , [IO.File]::ReadAllBytes($path) } else { return $null }
}

# --- The cases ---------------------------------------------------------------------------------------------

$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    foreach ($n in 'C3', 'C4', 'C20') {
      Write-ConformanceResult -Name "$n ($wherePlaces)" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
    }
  }
  else {
    $hostsPath = Join-Path $lab.UserProfile '.agentspace\hosts.json'
    $projectsPath = Join-Path $lab.UserProfile '.agentspace\projects.json'

    # ---- C3: swimcoach, no hosts.json: v0.06's bytes ---------------------------------------------------
    Reset-PgCase $lab
    $si = New-PgStandIn $lab
    $r = New-CcReadyLease -Lab $lab -Project 'swimcoach' -NoHost -EnvVars @{ AGENTSPACE_PROVISION_CMD = $si.Cmd }
    $starts = Wait-PgNoStart $si
    $bytes = if ($r.Ok) { Get-PgAliasBytes $lab $r.Doc } else { $null }
    Test-Cc "C3 ($aliasBox): A create for swimcoach with no hosts.json answers ready, and the alias file is v0.06's, byte for byte: HostKeyAlias agentspace and no UserKnownHostsFile" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($null -eq $bytes) { return 'There is no alias file by the document''s alias.' }
      $rest = Get-PgExpectedAliasRest $lab ([string]$r.Doc['id']) ([string]$r.Doc['ssh']['host_alias']) 'swimcoach' $true
      $p = Get-PgAliasProblems -Bytes $bytes -Id ([string]$r.Doc['id']) -ExpectedRest $rest
      if ($p.Count -gt 0) { return ($p -join '; ') + '.' }
      if ($utf8.GetString($bytes) -cmatch '(?m)^\s*UserKnownHostsFile\b') { return 'The box''s alias names a UserKnownHostsFile.' }
      $true
    }
    Test-Cc "C3 ($wherePlaces): The same create starts nothing and leaves hosts.json absent, and no project list is made" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s) within $($script:PgNegWaitMs) ms." }
      if (Test-Path -LiteralPath $hostsPath) { return 'hosts.json exists.' }
      if (Test-Path -LiteralPath $projectsPath) { return 'projects.json exists.' }
      $true
    }

    # ---- C3, second case: the box's form is decided by the record the lease is placed on ---------------
    Reset-PgCase $lab
    Write-PgHosts $lab @((New-PgHostRecord 'swimcoach' 'ready'), (New-PgHostRecord 'agentspace' 'ready'))
    $hostsBefore = Get-PgHostsHash $lab
    $si = New-PgStandIn $lab
    $r = New-CcReadyLease -Lab $lab -Project 'swimcoach' -NoHost -EnvVars @{ AGENTSPACE_PROVISION_CMD = $si.Cmd }
    $starts = Wait-PgNoStart $si
    $bytes = if ($r.Ok) { Get-PgAliasBytes $lab $r.Doc } else { $null }
    Test-Cc "C3 ($aliasOther): With another ready host in hosts.json, a swimcoach lease still has v0.06's bytes (the record whose stack is agentbox gives the box's form), starts nothing and leaves hosts.json as it was" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($null -eq $bytes) { return 'There is no alias file by the document''s alias.' }
      $rest = Get-PgExpectedAliasRest $lab ([string]$r.Doc['id']) ([string]$r.Doc['ssh']['host_alias']) 'swimcoach' $true
      $p = Get-PgAliasProblems -Bytes $bytes -Id ([string]$r.Doc['id']) -ExpectedRest $rest
      if ($p.Count -gt 0) { return ($p -join '; ') + '.' }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      if ((Get-PgHostsHash $lab) -cne $hostsBefore) { return 'hosts.json changed.' }
      $true
    }

    # ---- C4: a ready host that is not the box ------------------------------------------------------------
    Reset-PgCase $lab
    Write-PgHosts $lab @((New-PgHostRecord 'swimcoach' 'ready'), (New-PgHostRecord 'agentspace' 'ready'))
    $hostsBefore = Get-PgHostsHash $lab
    $si = New-PgStandIn $lab
    $r = New-CcReadyLease -Lab $lab -Project 'agentspace' -NoHost -EnvVars @{ AGENTSPACE_PROVISION_CMD = $si.Cmd }
    $starts = Wait-PgNoStart $si
    $bytes = if ($r.Ok) { Get-PgAliasBytes $lab $r.Doc } else { $null }
    Test-Cc "C4 ($aliasOther): A create for a project whose record is ready and is not the box answers ready, with HostKeyAlias agentspace-host.<project> and then UserKnownHostsFile, byte for byte, and no instance_id in the record" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($null -eq $bytes) { return 'There is no alias file by the document''s alias.' }
      $rest = Get-PgExpectedAliasRest $lab ([string]$r.Doc['id']) ([string]$r.Doc['ssh']['host_alias']) 'agentspace' $false
      $p = Get-PgAliasProblems -Bytes $bytes -Id ([string]$r.Doc['id']) -ExpectedRest $rest
      if ($p.Count -gt 0) { return ($p -join '; ') + '.' }
      $true
    }
    Test-Cc "C4 ($wherePlaces): The same create starts nothing, leaves hosts.json byte for byte as it was, and records the lease ready with its credential made" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s) within $($script:PgNegWaitMs) ms." }
      if ((Get-PgHostsHash $lab) -cne $hostsBefore) { return 'hosts.json changed.' }
      $rec = Get-CcRecord $lab ([string]$r.Doc['id'])
      if ($null -eq $rec -or $rec['state'] -cne 'ready') { return 'The lease state file holds no ready record for the lease.' }
      $dir = Get-CcLeaseDir $lab ([string]$r.Doc['id'])
      foreach ($f in 'id_ed25519', 'id_ed25519.pub', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path $dir $f) -PathType Leaf)) { return "The lease folder lacks $f." } }
      $true
    }
    # ssh is the oracle for what the file means: the host key's name, and the file that holds it.
    $g = @()
    if ($bytes) {
      try { $g = Get-CcSshG (Get-CcAliasPath $lab ([string]$r.Doc['ssh']['host_alias'])) ([string]$r.Doc['ssh']['host_alias']) } catch { $g = @('ssh could not be run') }
    }
    Test-Cc "C4 ($aliasOther): ssh -G reads the alias file: the host key is checked under agentspace-host.<project>, in agentspace's known_hosts, strictly" {
      if (-not $bytes) { return 'There is no alias file.' }
      $gt = $g -join "`n"
      if ($gt -cnotmatch '(?m)^hostkeyalias agentspace-host\.agentspace\s*$') { return 'hostkeyalias is not agentspace-host.agentspace.' }
      $known = (Join-Path $lab.UserProfile '.agentspace\known_hosts').Replace('\', '/')
      if ($gt -inotmatch "(?m)^userknownhostsfile `"?$([regex]::Escape($known))`"?\s*$") { return 'userknownhostsfile is not the full path of agentspace''s known_hosts.' }
      if ($gt -cnotmatch '(?m)^stricthostkeychecking true\s*$') { return 'stricthostkeychecking is not true.' }
      $true
    }

    # ---- C4, second case: the project is normalized before the lookup, and a hyphen is kept ---------------
    Reset-PgCase $lab
    Write-PgHosts $lab @((New-PgHostRecord 'swimcoach' 'ready'), (New-PgHostRecord 'lab-host' 'ready'))
    $si = New-PgStandIn $lab
    $r = New-CcReadyLease -Lab $lab -Project 'Lab-Host' -NoHost -EnvVars @{ AGENTSPACE_PROVISION_CMD = $si.Cmd }
    $starts = Wait-PgNoStart $si
    $bytes = if ($r.Ok) { Get-PgAliasBytes $lab $r.Doc } else { $null }
    Test-Cc "C4 ($aliasOther): A project given with capitals and a hyphen is normalized, and the alias names agentspace-host.lab-host" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($null -eq $bytes) { return 'There is no alias file by the document''s alias.' }
      if ($r.Doc['project'] -cne 'lab-host') { return 'The lease''s project is not lab-host.' }
      $rest = Get-PgExpectedAliasRest $lab ([string]$r.Doc['id']) ([string]$r.Doc['ssh']['host_alias']) 'lab-host' $false
      $p = Get-PgAliasProblems -Bytes $bytes -Id ([string]$r.Doc['id']) -ExpectedRest $rest
      if ($p.Count -gt 0) { return ($p -join '; ') + '.' }
      if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s)." }
      $true
    }

    # ---- C20: a ready host, another lease of the project pending ---------------------------------------------
    foreach ($v in @(
        @{ Name = 'with no last_attempt_at'; Minutes = $null },
        @{ Name = 'with a last_attempt_at 10 minutes old'; Minutes = -10 },
        @{ Name = 'with a last_attempt_at 1 minute old'; Minutes = -1 })) {
      Reset-PgCase $lab
      $at = if ($null -eq $v.Minutes) { '' } else { ConvertTo-CcTime (Get-CcNowSecond).AddMinutes($v.Minutes) }
      Write-PgHosts $lab @((New-PgHostRecord 'swimcoach' 'ready'), (New-PgHostRecord 'agentspace' 'ready' $at))
      $pending = New-PgPending 'agentspace'
      Write-PgLeases $lab @($pending)
      $hostsBefore = Get-PgHostsHash $lab
      $pendingBefore = ConvertTo-CcCanonical (Get-CcRecord $lab $pending['id'])
      $si = New-PgStandIn $lab
      $r = New-CcReadyLease -Lab $lab -Project 'agentspace' -NoHost -EnvVars @{ AGENTSPACE_PROVISION_CMD = $si.Cmd }
      $starts = Wait-PgNoStart $si
      Test-Cc "C20 ($wherePlaces): A create for a ready host's project, while another lease of it is pending, answers ready and starts nothing, $($v.Name), and changes neither the registry nor the pending lease" {
        if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
        if ($starts.Count -ne 0) { return "The stand-in was started $($starts.Count) time(s) within $($script:PgNegWaitMs) ms." }
        if ((Get-PgHostsHash $lab) -cne $hostsBefore) { return 'hosts.json changed.' }
        $now = Get-CcRecord $lab $pending['id']
        if ($null -eq $now) { return 'The pending lease''s record is gone.' }
        if ((ConvertTo-CcCanonical $now) -cne $pendingBefore) { return 'The pending lease''s record changed.' }
        if ($r.Doc['id'] -ceq $pending['id']) { return 'The create answered with the pending lease.' }
        $true
      }
    }
  }
}
catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-alias-bytes.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
}
finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

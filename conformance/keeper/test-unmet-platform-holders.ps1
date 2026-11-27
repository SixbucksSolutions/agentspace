#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md and docs/state.md document, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# THE KEEPER'S CASES FOR A HOST PER PROJECT AND PLATFORM (v0.12, docs/keeper.md, "A host's key and name",
# "Which host a lease is on", "The hosts of a pass", "Stopping a holder", "Fast failures and backoff", "The
# holder's log", and "Conformance cases for a host per project and platform", cases H19 to H29, written by
# E0.12-T04-S03, card #0263; architect, 2026-11-26, for v0.12-T04, pin P9: all of them in this one file).
# It checks behavior that v0.11's keeper, which knows one host per project, does not have, so it is expected
# to FAIL against v0.11's keeper in every case but H24 and H29a, which describe what v0.11 already did (a name
# with a dot in it is no record, and a registry with no platform in it reads as it did); a v0.12 keeper MUST
# pass every line.
#
# Each case runs against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the harness)
# with the keeper's own arguments unchanged: <program> -SshExe <stub> -Now <time>, or with -Watch. Each runs
# with a throwaway USERPROFILE and state folder, a stub -SshExe, a stand-in guard (AGENTSPACE_GUARD_CMD) and
# tripwires for aws, ssh and scp (keeper-common.ps1's New-CaseHome names all four), and a hosts.json the case
# writes under the throwaway USERPROFILE (never the state folder), or none. <p> is agentspace and <q> is
# zeta, as in test-unmet-hosts-pass.ps1; the box's project is swimcoach. No case needs a real host: a
# holder's connection is a stub's. Every expectation below is keeper.md's or state.md's text, never a
# program's output; a log line is matched as keeper.md gives it.
#
# The host names the cases write, as keeper.md, "A host's key and name", gives them: <p> for a linux/arm64
# host, <p>.linux-amd64 and <p>.windows-amd64 for the others. A lease's alias is agentspace-<project>-<8 hex
# digits> on every platform (pin P7), so the stand-in tells two hosts of one project apart by the lease it
# is started for, never by the alias's text.
#
# H29. keeper.md's H29 row says every v0.11 case of the keeper, run against a registry whose records all
# name linux/arm64, or none a platform, written in the ordinal order of their projects after the box's, behaves
# as it did. THIS FILE SHOWS IT WITH TWO CASES OF ITS OWN, and does not rerun the v0.11 files:
# keeper-common.ps1's registry mode (-Registry None|Box|Both) writes the box's record alone and has no mode
# for a registry of other hosts, and keeper-common.ps1 is not this file's to change.
#   - H29a: a registry of the box, <p> and <q>, in that order (the ordinal order of the projects after the
#     box), no record naming a platform: three holders are started, kept and one stopped, line for line as v0.11
#     did (host= fields that are a project and nothing else, v0.11's file names, no platform member in any
#     holder record, each launch request's log named for its project).
#   - H29b: the same scenario with every record, the box's included, holding "platform":"linux/arm64"
#     (state.md, "A host's platform and names": a reader takes it as equal to no member).
# What they do not cover: the v0.06 cases of test-keeper.ps1 and its three sister files, which run in
# -Registry Box (the box's record alone, no platform); H2 to H18 (test-unmet-hosts-pass.ps1 and
# test-unmet-hosts-watch.ps1), whose registries already hold the box, <p> and <q> in ordinal order with no
# platform member; and G1 to G16 (test-unmet-keeper-guard.ps1). A rerun of those files against a v0.12 build is a
# lane's, and the architect's.
#
# What it touches: the same as test-unmet-hosts-pass.ps1 (conformance/keeper/keeper-common.ps1, and this
# suite's conformance/keeper/keeper-hosts-common.ps1, which this file loads and does not change): a throwaway
# folder per case under $env:TEMP, the harness's child environment for it on every process, the stand-in
# ssh-hosts-stub.cmd (written by the run into its own folder), and only processes this run started, each
# stopped by a checked PID. The real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe, keeper task and box are never
# read, written, listed or hashed. Every write and delete goes through Assert-ConformancePath at the call.
# Every helper this file defines carries Ph in its name.
#
# Every wait is on what a case checks for, with a deadline, never a fixed sleep (keeper.md, "Checking the
# keeper", card #0066).
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-platform-holders.ps1 [-Case <wildcard on a case's ID>]
param([string]$Case = '*')
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) {
  Write-Output 'SKIPPED keeper.md#where-it-runs: The keeper specified is a Windows keeper; this host is not Windows.'
  Write-Output '0 passed, 0 failed, 1 skipped'
  exit 2
}

. (Join-Path $PSScriptRoot 'keeper-common.ps1')
. (Join-Path $PSScriptRoot 'keeper-hosts-common.ps1')

# Resolved before the first case, which strips AGENTSPACE_* from the environment.
$keeperCmd = Resolve-KeeperProgram
if (-not $keeperCmd -or -not (Test-Path -LiteralPath $keeperCmd -PathType Leaf)) {
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { "$(ConvertTo-ConformanceMasked $keeperCmd) is not a file." } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun
Initialize-HostsRun

# A case whose id is not matched by -Case is not run.
function Test-Selected([string]$Id) { $Id -like $Case }

$phArm = 'linux/arm64'
$phLinux = 'linux/amd64'
$phWin = 'windows/amd64'
$projectPattern = '\A[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?\z'

# ---- Names, records and leases ----

# A host's name (keeper.md, "A host's key and name"): the project for linux/arm64, else the project, a dot and the
# platform with its one / written -.
function Get-PhHostName([string]$Project, [string]$Platform = 'linux/arm64') {
  if ($Platform -ceq 'linux/arm64') { $Project } else { $Project + '.' + $Platform.Replace('/', '-') }
}

# A record of the host registry (state.md, "The host registry"): platform after project and before stack, for a host
# that is not linux/arm64 or when -Explicit asks for it; the box's stack is agentbox, a linux/arm64 host's
# agentspace-host-<project>, any other's agentspace-<os>-<arch>-host-<project>.
function New-PhRecord([string]$Project, [string]$Platform = 'linux/arm64', [string]$State = 'ready', [switch]$Explicit) {
  $record = [ordered]@{ project = $Project }
  if ($Platform -cne 'linux/arm64' -or $Explicit) { $record['platform'] = $Platform }
  $record['stack'] = if ($Platform -ceq 'linux/arm64') { if ($Project -ceq $boxProject) { 'agentbox' } else { "agentspace-host-$Project" } } else { 'agentspace-' + $Platform.Replace('/', '-') + "-host-$Project" }
  $record['state'] = $State
  $record
}

# A lease on a platform (state.md, "The lease state file"): the platform's default profile and its user (ubuntu on
# either Linux platform, agent on windows/amd64). Its ID and alias are made from its project and a two-hex-digit
# tag, as keeper-hosts-common.ps1's are.
function New-PhLease([string]$Project, [string]$Platform, [string]$Tag, [string]$Expires, [switch]$Released) {
  $profileName = if ($Platform -ceq 'windows/amd64') { 'agent-windows' } else { 'agent-linux' }
  $userName = if ($Platform -ceq 'windows/amd64') { 'agent' } else { 'ubuntu' }
  $lease = [ordered]@{
    id = Get-HostLeaseId $Tag; project = $Project; state = $(if ($Released) { 'released' } else { 'ready' }); platform = $Platform; profile = $profileName
    size = 'm'; ttl_seconds = 28800; created_at = $created; expires_at = $Expires
  }
  if ($Released) { $lease['released_at'] = '2026-11-17T09:00:00Z' }
  else { $lease['ssh'] = [ordered]@{ host_alias = Get-HostAlias $Project $Tag; user = $userName } }
  $lease
}

# The holder's record, written as state.md gives it: pid, process_started_at, lease_id, alias, and platform last when
# -Platform is given. -Path names the file, so that a case can write one that is not a host's record.
function Write-PhHolderRecord([string]$Path, [int]$ProcessId, [string]$StartedAt, [string]$LeaseId, [string]$Alias, [string]$Platform = '') {
  $record = [ordered]@{ pid = $ProcessId; process_started_at = $StartedAt; lease_id = $LeaseId; alias = $Alias }
  if ($Platform) { $record['platform'] = $Platform }
  [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $Path), (($record | ConvertTo-Json -Compress) + "`n"))
}

# A running holder the keeper would trust on a host, and its record: a stand-in with the holder's arguments for the
# lease's alias, and the host's record naming it. -FileName names another file than the host's own.
function Start-PhHolder([string]$Project, [string]$Platform, [string]$Tag, [string]$FileName = '', [string]$RecordPlatform = '<default>') {
  $alias = Get-HostAlias $Project $Tag
  $proc = Start-Stub $alias
  $path = if ($FileName) { Join-Path $env:AGENTSPACE_STATE_DIR $FileName } else { Get-HostRecordPath (Get-PhHostName $Project $Platform) }
  $member = if ($RecordPlatform -ceq '<default>') { if ($Platform -ceq 'linux/arm64') { '' } else { $Platform } } else { $RecordPlatform }
  Write-PhHolderRecord $path $proc.Pid $proc.Started (Get-HostLeaseId $Tag) $alias $member
  $proc
}

# The names of every holder record of a host that is not the box, as the state folder holds them: board-holder.<name>.json,
# where <name> is a project or a project, a dot and more.
function Get-PhRecordNames { @(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder\.[a-z0-9.-]+\.json\z' }) }

# Forgets what a pass did: every holder, and the keeper's records, logs and holders' logs, dotted names included.
function Reset-PhState {
  Stop-RunProcesses
  foreach ($item in @(Get-ChildItem -LiteralPath $env:AGENTSPACE_STATE_DIR -Force -ErrorAction SilentlyContinue)) {
    if ($item.Name -cmatch '\Aboard-holder(\.[a-z0-9.-]+)?\.json\z' -or $item.Name -ceq 'board-holder.log' -or $item.Name -cmatch '\Aboard-holder-ssh(\.[a-z0-9.-]+)?\.log(\.1)?\z') {
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $runHome -Path $item.FullName) -Force -ErrorAction SilentlyContinue
    }
  }
}

# The host= field of every line that has one, in order.
function Get-PhLineHosts($Lines) {
  @($Lines | ForEach-Object { if ($_ -cmatch ' host=(\S+)(\s|\z)') { $Matches[1] } })
}

# The host= fields of the lines of one action, in order.
function Get-PhActionHosts($Lines, [string]$Action) {
  @(Select-ActionLines $Lines $Action | ForEach-Object { if ($_ -cmatch ' host=(\S+)(\s|\z)') { $Matches[1] } else { '-' } })
}

# True when a holder record's text holds the four members v0.11 wrote and no platform member.
function Test-PhV011Record([string]$Text) {
  $parsed = $null
  try { $parsed = $Text | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  if ($null -eq $parsed -or $parsed.Count -ne 4) { return $false }
  foreach ($member in 'pid', 'process_started_at', 'lease_id', 'alias') { if (-not $parsed.ContainsKey($member)) { return $false } }
  -not $Text.Contains('platform')
}

try {
  # H19. A project's linux/arm64 and windows/amd64 hosts: two holders, one through each lease, never one through both.
  if (Test-Selected 'H19') { Invoke-HostCase 'H19' 'keeper.md#a-hosts-key-and-name' {
    $hostWin = Get-PhHostName $pProject $phWin
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phWin))
    # The windows lease expires later, so a keeper that chose one lease for both of <p>'s hosts would choose it for both.
    Write-Leases @((New-PhLease $pProject $phArm 'c1' '2026-11-17T13:00:00Z'), (New-PhLease $pProject $phWin 'c2' '2026-11-17T14:00:00Z'))
    $pass = Invoke-CapturedPass $now 2
    Add-HostCheck ($pass.Code -eq 0) 'The pass exits 0' "The exit code was $($pass.Code)."
    Add-HostCheck $pass.Complete 'Two starts are made, each with its request, claim and result seen' "Starts seen: $(@($pass.Tokens).Count), in $($pass.Attempts) passes."
    $recArm = Read-HostRecord $pProject
    $recWin = Read-HostRecord $hostWin
    $log = Read-LogLines
    $started = @(Select-ActionLines $log 'started')
    Add-HostCheck ($log.Count -eq 3 -and $started.Count -eq 2 -and (Test-IdleLine $log[0])) 'The box''s turn comes first and logs idle with no host, and two started lines follow it, and nothing else' "Log: $($log -join ' | ')"
    Add-HostCheck ($started.Count -ge 1 -and $null -ne $recArm -and (Test-HostLine $started[0] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recArm['pid']) $pProject)) 'The first started line is the linux/arm64 host''s, through its own lease, ending in host=agentspace' "Started lines: $($started -join ' | ')"
    Add-HostCheck ($started.Count -ge 2 -and $null -ne $recWin -and (Test-HostLine $started[1] 'started' (Get-HostLeaseId 'c2') (Get-HostAlias $pProject 'c2') ([string]$recWin['pid']) $hostWin)) 'The second started line is the windows/amd64 host''s, through its own lease, ending in host=agentspace.windows-amd64' "Started lines: $($started -join ' | ')"
    Add-HostCheck ($null -ne $recArm -and $recArm['lease_id'] -ceq (Get-HostLeaseId 'c1') -and -not $recArm.ContainsKey('platform')) 'board-holder.agentspace.json names the linux/arm64 lease and holds no platform member' 'The record is absent, names another lease, or holds a platform member.'
    Add-HostCheck ($null -ne $recWin -and $recWin['lease_id'] -ceq (Get-HostLeaseId 'c2') -and $recWin['platform'] -ceq $phWin) 'board-holder.agentspace.windows-amd64.json names the windows/amd64 lease and holds platform windows/amd64' 'The record is absent, names another lease, or holds another platform.'
    Add-HostCheck (Test-RequestLog (Get-CapturedRequest $pass 0) 'board-holder-ssh.agentspace.log') 'The first start''s launch request names board-holder-ssh.agentspace.log' 'The first request names another log, or none was seen.'
    Add-HostCheck (Test-RequestLog (Get-CapturedRequest $pass 1) 'board-holder-ssh.agentspace.windows-amd64.log') 'The second start''s launch request names board-holder-ssh.agentspace.windows-amd64.log' 'The second request names another log, or none was seen.'
    $cmdArm = if ($recArm) { Get-CommandLine ([int]$recArm['pid']) } else { '' }
    $cmdWin = if ($recWin) { Get-CommandLine ([int]$recWin['pid']) } else { '' }
    Add-HostCheck ($null -ne $recArm -and $null -ne $recWin -and [int]$recArm['pid'] -ne [int]$recWin['pid'] -and (Test-Alive ([int]$recArm['pid'])) -and (Test-Alive ([int]$recWin['pid'])) -and $cmdArm.Contains((Get-HostAlias $pProject 'c1')) -and -not $cmdArm.Contains((Get-HostAlias $pProject 'c2')) -and $cmdWin.Contains((Get-HostAlias $pProject 'c2')) -and -not $cmdWin.Contains((Get-HostAlias $pProject 'c1'))) 'Two different holders run, each for its own host''s lease and never the other''s' "Linux/arm64 holder: $cmdArm; windows/amd64 holder: $cmdWin"
    Add-HostCheck ((Get-HolderProcesses).Count -eq 2 -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'))) 'Two holders only, and no box record' "Holders found: $((Get-HolderProcesses).Count)."
  } }

  # H20. The turns follow the registry's file order, and no sorted order.
  if (Test-Selected 'H20') { Invoke-HostCase 'H20' 'keeper.md#the-hosts-of-a-pass' {
    # In file order after the box's: <q> linux/amd64, <p> linux/arm64, <q> linux/arm64, <p> linux/amd64. <p> (agentspace) is before <q> (zeta) in ordinal order.
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $qProject $phLinux), (New-PhRecord $pProject), (New-PhRecord $qProject), (New-PhRecord $pProject $phLinux))
    Write-Leases @(
      (New-PhLease $qProject $phLinux 'd1' '2026-11-17T13:00:00Z'), (New-PhLease $pProject $phArm 'c1' '2026-11-17T13:00:00Z'),
      (New-PhLease $qProject $phArm 'd2' '2026-11-17T13:00:00Z'), (New-PhLease $pProject $phLinux 'c2' '2026-11-17T13:00:00Z'))
    $pass = Invoke-CapturedPass $now 4
    Add-HostCheck ($pass.Code -eq 0) 'The pass exits 0' "The exit code was $($pass.Code)."
    Add-HostCheck $pass.Complete 'Four starts are made, each with its request, claim and result seen' "Starts seen: $(@($pass.Tokens).Count), in $($pass.Attempts) passes."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 5 -and (Test-IdleLine $log[0])) 'The box''s turn comes first, and logs idle with no host' "Log: $($log -join ' | ')"
    $wanted = @('zeta.linux-amd64', 'agentspace', 'zeta', 'agentspace.linux-amd64')
    $hosts = @(Get-PhActionHosts $log 'started')
    Add-HostCheck (($hosts -join ',') -ceq ($wanted -join ',')) 'The started lines'' host= fields read zeta.linux-amd64, agentspace, zeta, agentspace.linux-amd64, in that order: the registry''s file order, never a sorted one' "The host= fields, in the order logged: $($hosts -join ', ')"
    $expect = @(@($qProject, $phLinux, 'd1'), @($pProject, $phArm, 'c1'), @($qProject, $phArm, 'd2'), @($pProject, $phLinux, 'c2'))
    $started = @(Select-ActionLines $log 'started')
    $okLines = $started.Count -eq 4
    for ($i = 0; $okLines -and $i -lt 4; $i++) {
      $name = Get-PhHostName $expect[$i][0] $expect[$i][1]
      $rec = Read-HostRecord $name
      if ($null -eq $rec -or -not (Test-HostLine $started[$i] 'started' (Get-HostLeaseId $expect[$i][2]) (Get-HostAlias $expect[$i][0] $expect[$i][2]) ([string]$rec['pid']) $name)) { $okLines = $false }
    }
    Add-HostCheck $okLines 'Each started line names its own host''s lease, alias and holder, the one its record names' "Started lines: $($started -join ' | ')"
  } }

  # H21. A lease whose platform's host is not ready is on no host, never the box.
  if (Test-Selected 'H21') { Invoke-HostCase 'H21' 'keeper.md#which-host-a-lease-is-on' {
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phLinux 'creating'))
    # The linux/amd64 lease expires later: a keeper that put it on a host would choose it.
    Write-Leases @((New-PhLease $pProject $phLinux 'c2' '2026-11-17T14:00:00Z'), (New-PhLease $pProject $phArm 'c1' '2026-11-17T13:00:00Z'))
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $recArm = Read-HostRecord $pProject
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 2 -and (Test-IdleLine $log[0]) -and $null -ne $recArm -and (Test-HostLine $log[1] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recArm['pid']) $pProject)) 'The box''s idle line, then one started line for the linux/arm64 host through the linux/arm64 lease' "Log: $($log -join ' | ')"
    Add-HostCheck ($null -ne $recArm -and $recArm['lease_id'] -ceq (Get-HostLeaseId 'c1')) 'board-holder.agentspace.json names the linux/arm64 lease' 'The record is absent or names another lease.'
    Add-HostCheck (-not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder.json'))) 'The linux/amd64 lease is not on the box: the box has no record' 'board-holder.json was written.'
    Add-HostCheck (@($log | Where-Object { $_ -cmatch 'amd64' }).Count -eq 0 -and @(Get-StateNames | Where-Object { $_ -cmatch 'amd64' }).Count -eq 0) 'No line, record or log of agentspace.linux-amd64 appears' "State folder: $((Get-StateNames) -join ', '). Log: $($log -join ' | ')"
    Add-HostCheck ((Get-HolderProcesses).Count -eq 1) 'One holder only' "Holders found: $((Get-HolderProcesses).Count)."
  } }

  # H22. With no registry, a lease of a platform other than linux/arm64 is on no host: the box does not take it.
  if (Test-Selected 'H22') { Invoke-HostCase 'H22' 'keeper.md#which-host-a-lease-is-on' {
    foreach ($platform in $phWin, $phLinux) {
      Reset-PhState
      Write-Leases @((New-PhLease $pProject $platform 'c1' '2026-11-17T13:00:00Z'))
      $label = "A $platform lease with no registry"
      $code = Invoke-Keeper $now
      Add-HostCheck ($code -eq 0) "$label`: The pass exits 0" "The exit code was $code."
      $log = Read-LogLines
      Add-HostCheck ($log.Count -eq 1 -and (Test-IdleLine $log[0])) "$label`: The pass logs only the box's idle lease=- alias=- pid=-" "Log: $($log -join ' | ')"
      Add-HostCheck ((Get-HolderProcesses).Count -eq 0 -and @(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder.*\.json\z' }).Count -eq 0) "$label`: No holder is started and no holder record appears, the box's and agentspace.$($platform.Replace('/', '-'))'s included" "State folder: $((Get-StateNames) -join ', '). Holders found: $((Get-HolderProcesses).Count)."
    }
  } }

  # H23. A holder of a ready host with no ready lease on its platform is stopped, in a keeper with its grace; another host's holder is kept.
  if (Test-Selected 'H23') { Invoke-HostCase 'H23' 'keeper.md#stopping-a-holder' {
    $hostWin = Get-PhHostName $pProject $phWin
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phWin))
    # <p>'s windows/amd64 host is ready, and its only lease on that platform is released: no ready lease is on it.
    Write-Leases @((New-PhLease $pProject $phArm 'c1' $farFuture), (New-PhLease $pProject $phWin 'c2' $farFuture -Released))
    $holdArm = Start-PhHolder $pProject $phArm 'c1'
    $holdWin = Start-PhHolder $pProject $phWin 'c2'
    $recArmPath = Get-HostRecordPath $pProject
    $recArmBefore = [IO.File]::ReadAllText($recArmPath)
    $graceSeconds = 15
    $proc = Start-Watch @('-IntervalSeconds', '1', '-BackoffBaseSeconds', '1', '-BackoffMaxSeconds', '1', '-FastFailureSeconds', '600', '-ReleaseGraceSeconds', "$graceSeconds") $stub
    try {
      $graced = Wait-Until { @(Get-HostGraces (Read-LogLines) $hostWin).Count -ge 1 } 90 $proc
      $gone = Wait-Until { -not (Test-Alive $holdWin.Pid) } 30
    } finally { Stop-Watch $proc }
    $log = Read-LogLines
    Add-HostCheck ($log.Count -ge 4 -and (Test-IdleLine $log[0]) -and (Test-HostLine $log[1] 'kept' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$holdArm.Pid) $pProject)) 'The first pass''s turns come in the registry''s order: the box''s idle, then kept for the linux/arm64 holder, host=agentspace' "Log: $(($log | Select-Object -First 4) -join ' | ')"
    Add-HostCheck ($log.Count -ge 4 -and (Test-HostLine $log[2] 'stopped' (Get-HostLeaseId 'c2') (Get-HostAlias $pProject 'c2') ([string]$holdWin.Pid) $hostWin)) 'The same pass logs stopped for the windows/amd64 holder, ending in host=agentspace.windows-amd64' "Log: $(($log | Select-Object -First 4) -join ' | ')"
    $g = @(Get-HostGraces $log $hostWin)
    Add-HostCheck ($graced -and $g[0] -ge ($graceSeconds - 2) -and $g[0] -le $graceSeconds -and $log.Count -ge 4 -and $log[3] -cmatch ($logTime + 'grace seconds=\d+ host=agentspace\.windows-amd64\z')) "The keeper then logs grace seconds=$graceSeconds less rounding, with host=agentspace.windows-amd64" "Grace lines for the windows/amd64 host: $($g -join ','). Log: $(($log | Select-Object -First 5) -join ' | ')"
    Add-HostCheck (@(Get-HostGraces $log '').Count -eq 0 -and @(Get-HostGraces $log $pProject).Count -eq 0) 'No other host has a grace line' "Grace lines: $(@($log | Where-Object { $_ -cmatch ' grace seconds=' }) -join ' | ')"
    Add-HostCheck ($gone -and -not (Test-Path -LiteralPath (Get-HostRecordPath $hostWin))) 'The record board-holder.agentspace.windows-amd64.json is removed and the stub''s process is gone' 'The record is still there, or the stub is still running.'
    Add-HostCheck ((Test-TreeAlive $holdArm) -and [IO.File]::ReadAllText($recArmPath) -ceq $recArmBefore) 'The linux/arm64 holder is still running and its record is unchanged' 'It was stopped or its record changed.'
  } }

  # H24. A name that is no host's record is never read, removed or stopped.
  if (Test-Selected 'H24') { Invoke-HostCase 'H24' 'keeper.md#the-hosts-of-a-pass' {
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phWin))
    Write-Leases @()
    $files = @("board-holder.$pProject.linux-arm64.json", "board-holder.$pProject.windows-arm64.json", "board-holder.$pProject.windows-amd64.extra.json", "board-holder.$pProject.-amd64.json")
    foreach ($fileName in $files) {
      Reset-PhState
      $stray = Join-Path $env:AGENTSPACE_STATE_DIR $fileName
      $holdStray = Start-PhHolder $pProject $phWin 'c1' $fileName
      $before = [IO.File]::ReadAllText($stray)
      $code = Invoke-Keeper $now
      $label = "$fileName"
      Add-HostCheck ($code -eq 0) "$label`: The pass exits 0" "The exit code was $code."
      $log = Read-LogLines
      Add-HostCheck ($log.Count -eq 1 -and (Test-IdleLine $log[0])) "$label`: One idle line is logged, with no host, and no line names the file or its holder" "Log: $($log -join ' | ')"
      Add-HostCheck ((Test-Path -LiteralPath $stray) -and [IO.File]::ReadAllText($stray) -ceq $before) "$label`: The file is not removed or changed" 'The file was removed or changed.'
      Add-HostCheck (Test-TreeAlive $holdStray) "$label`: The process it names is left running" 'The process was stopped.'
    }
  } }

  # H25. A record whose platform is not its file's host's platform is no record.
  if (Test-Selected 'H25') { Invoke-HostCase 'H25' 'keeper.md#trusting-the-record' {
    $hostWin = Get-PhHostName $pProject $phWin
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject $phWin))
    Write-Leases @((New-PhLease $pProject $phWin 'c1' '2026-11-17T13:00:00Z'))
    # The stub runs for the lease's alias, so that only the record's platform makes the record untrustworthy.
    $holdOld = Start-PhHolder $pProject $phWin 'c1' '' $phLinux
    $oldPid = $holdOld.Pid
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $recNew = Read-HostRecord $hostWin
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 2 -and (Test-IdleLine $log[0]) -and $null -ne $recNew -and (Test-HostLine $log[1] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recNew['pid']) $hostWin)) 'The box''s idle line, then a started line for the windows/amd64 host: the holder is not kept' "Log: $($log -join ' | ')"
    Add-HostCheck ($null -ne $recNew -and [int]$recNew['pid'] -ne $oldPid -and $recNew['platform'] -ceq $phWin) 'The record is now the new holder''s, with platform windows/amd64' 'The record is the old one, or absent, or holds another platform.'
    Add-HostCheck (Test-TreeAlive $holdOld) 'The stub the untrusted record named is left running' 'It was stopped.'
  } }

  # H26. The box's project's other platform has its own record, never read as the box's.
  if (Test-Selected 'H26') { Invoke-HostCase 'H26' 'keeper.md#the-hosts-of-a-pass' {
    $hostWin = Get-PhHostName $boxProject $phWin
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $boxProject $phWin))
    Write-Leases @((New-PhLease $boxProject $phArm 'b1' $farFuture), (New-PhLease $boxProject $phWin 'b2' $farFuture))
    $holdBox = Start-PhHolder $boxProject $phArm 'b1'
    $holdWin = Start-PhHolder $boxProject $phWin 'b2'
    $pathBox = Get-HostRecordPath $boxProject
    $pathWin = Get-HostRecordPath $hostWin
    $beforeBox = [IO.File]::ReadAllText($pathBox)
    $beforeWin = [IO.File]::ReadAllText($pathWin)
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 2 -and (Test-HostLine $log[0] 'kept' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$holdBox.Pid) $boxProject)) 'The box''s turn keeps the holder board-holder.json names, ending in host=swimcoach' "Log: $($log -join ' | ')"
    Add-HostCheck ($log.Count -eq 2 -and (Test-HostLine $log[1] 'kept' (Get-HostLeaseId 'b2') (Get-HostAlias $boxProject 'b2') ([string]$holdWin.Pid) $hostWin)) 'The other host''s turn keeps the holder board-holder.swimcoach.windows-amd64.json names, ending in host=swimcoach.windows-amd64' "Log: $($log -join ' | ')"
    Add-HostCheck ((Test-TreeAlive $holdBox) -and (Test-TreeAlive $holdWin) -and [IO.File]::ReadAllText($pathBox) -ceq $beforeBox -and [IO.File]::ReadAllText($pathWin) -ceq $beforeWin) 'Both holders are still running and both records are unchanged' 'A holder was stopped or a record changed.'
  } }

  # H27. A windows/amd64 host's holder log over 1 MiB is rotated, in the ordinal order of the host names.
  if (Test-Selected 'H27') { Invoke-HostCase 'H27' 'keeper.md#the-holders-log' {
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phWin))
    Write-Leases @()
    $big = 1MB + 100
    $small = 1MB - 100
    # Ordinal order of the host names: agentspace, agentspace-x, agentspace.linux-amd64, agentspace.windows-amd64, agentspace2
    # ('-' is U+002D, '.' U+002E and '2' U+0032). Written in another order, so that no order of creation or of listing gives it.
    $rotatedNames = @('agentspace2', 'agentspace.windows-amd64', 'agentspace', 'agentspace.linux-amd64', 'agentspace-x')
    $specs = @(, @('board-holder-ssh.log', $big))
    foreach ($name in $rotatedNames) { $specs += , @("board-holder-ssh.$name.log", $big) }
    $specs += , @("board-holder-ssh.$pProject.linux-arm64.log", $big)
    $specs += , @("board-holder-ssh.$pProject.windows-arm64.log", $big)
    $specs += , @("board-holder-ssh.$qProject.log", $small)
    foreach ($spec in $specs) {
      $fs = [IO.File]::Create((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $env:AGENTSPACE_STATE_DIR $spec[0])))
      try { $fs.SetLength($spec[1]) } finally { $fs.Dispose() }
    }
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    $wantedHosts = @('agentspace', 'agentspace-x', 'agentspace.linux-amd64', 'agentspace.windows-amd64', 'agentspace2')
    $rotated = @(Select-ActionLines $log 'rotated')
    $hostsRotated = @(Get-PhActionHosts $log 'rotated')
    Add-HostCheck ($rotated.Count -eq 6 -and $rotated[0] -cmatch ($logTime + 'rotated lease=- alias=- pid=-\z') -and ($hostsRotated[1..5] -join ',') -ceq ($wantedHosts -join ',')) 'The box''s rotated line (no host) comes first, then rotated host=agentspace, agentspace-x, agentspace.linux-amd64, agentspace.windows-amd64 and agentspace2, in the ordinal order of the host names' "Rotated lines: $($rotated -join ' | ')"
    $allOk = $rotated.Count -eq 6
    foreach ($wantedHost in $wantedHosts) {
      if (-not ($rotated | Where-Object { $_ -cmatch ($logTime + 'rotated lease=- alias=- pid=- host=' + [regex]::Escape($wantedHost) + '\z') })) { $allOk = $false }
    }
    Add-HostCheck ($allOk -and $log.Count -eq 7 -and $rotated.Count -eq 6 -and (Test-IdleLine $log[6])) 'Each rotated line reads rotated lease=- alias=- pid=- host=<host name>, and the box''s idle line follows them' "Log: $($log -join ' | ')"
    $old = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.agentspace.windows-amd64.log.1'))
    Add-HostCheck ($old.Exists -and $old.Length -eq $big -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.agentspace.windows-amd64.log'))) 'board-holder-ssh.agentspace.windows-amd64.log is renamed to board-holder-ssh.agentspace.windows-amd64.log.1, whole' 'The log was not renamed.'
    $renamedOk = $true
    foreach ($name in $rotatedNames) {
      $info = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$name.log.1"))
      if (-not ($info.Exists -and $info.Length -eq $big -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$name.log")))) { $renamedOk = $false }
    }
    $boxOld = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log.1'))
    Add-HostCheck ($renamedOk -and $boxOld.Exists -and $boxOld.Length -eq $big) 'Every other host''s log, and the box''s, is renamed to its .log.1, whole' 'A log was not renamed.'
    $leftOk = $true
    foreach ($name in "$pProject.linux-arm64", "$pProject.windows-arm64") {
      $info = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$name.log"))
      if (-not ($info.Exists -and $info.Length -eq $big -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$name.log.1")))) { $leftOk = $false }
    }
    $under = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$qProject.log"))
    Add-HostCheck ($leftOk -and $under.Exists -and $under.Length -eq $small -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR "board-holder-ssh.$qProject.log.1"))) 'A log whose name is no host''s (linux-arm64, windows-arm64) and a host''s log under 1 MiB are left alone' 'One of them was rotated.'
  } }

  # H28. One host's failures, backoffs and graces are its own: the windows/amd64 host backs off alone.
  if (Test-Selected 'H28') { Invoke-HostCase 'H28' 'keeper.md#fast-failures-and-backoff' {
    $hostWin = Get-PhHostName $pProject $phWin
    Write-Registry @((New-PhRecord $boxProject), (New-PhRecord $pProject), (New-PhRecord $pProject $phWin))
    Write-Leases @((New-PhLease $pProject $phArm 'c1' $farFuture), (New-PhLease $pProject $phWin 'c2' $farFuture))
    Set-FailMarker (Get-HostAlias $pProject 'c2')
    $proc = Start-Watch @('-IntervalSeconds', '1', '-BackoffBaseSeconds', '2', '-BackoffMaxSeconds', '8') $stub
    try {
      $got = Wait-Until { @(Get-HostBackoffs (Read-LogLines) $hostWin).Count -ge 3 } 120 $proc
    } finally { Stop-Watch $proc }
    $log = Read-LogLines
    $backoffs = @(Get-HostBackoffs $log $hostWin)
    Add-HostCheck $got 'The keeper logs three backoff lines for the windows/amd64 host' "Backoff lines seen for it: $($backoffs.Count). The log ends: $(($log | Select-Object -Last 4) -join ' | ')"
    Add-HostCheck ($backoffs.Count -ge 3 -and (($backoffs[0..2]) -join ',') -ceq '2,4,8') "The windows/amd64 host's backoff lines say seconds 2, 4, 8 (got $($backoffs -join ','))"
    $holdOffLines = @($log | Where-Object { $_ -cmatch ' (backoff|grace) seconds=' })
    Add-HostCheck ($holdOffLines.Count -gt 0 -and @($holdOffLines | Where-Object { $_ -cmatch ' (backoff|grace) seconds=\d+ host=agentspace\.windows-amd64\z' }).Count -eq $holdOffLines.Count) 'Every backoff and grace line is the windows/amd64 host''s: none is the linux/arm64 host''s or the box''s' "Lines: $($holdOffLines -join ' | ')"
    # keeper.md's H9 row: each fast failure is a failed line or a started line whose holder a later look finds gone, followed by that host's next backoff line.
    $startedRx = $logTime + 'started lease=' + [regex]::Escape((Get-HostLeaseId 'c2')) + ' alias=' + [regex]::Escape((Get-HostAlias $pProject 'c2')) + ' pid=\d+ host=agentspace\.windows-amd64\z'
    $segments = [System.Collections.Generic.List[object]]::new()
    $current = [System.Collections.Generic.List[string]]::new()
    foreach ($line in $log) {
      if ($line -cmatch ' backoff seconds=\d+ host=agentspace\.windows-amd64\z') { $segments.Add(@($current.ToArray())); $current = [System.Collections.Generic.List[string]]::new() }
      elseif (Test-HostLine $line 'failed' (Get-HostLeaseId 'c2') (Get-HostAlias $pProject 'c2') '-' $hostWin) { $current.Add('failed') }
      elseif ($line -cmatch $startedRx) { $current.Add('started') }
    }
    $firstThree = @($segments | Select-Object -First 3)
    $forms = @($firstThree | ForEach-Object { @($_) -join '+' })
    Add-HostCheck ($firstThree.Count -eq 3 -and @($firstThree | Where-Object { @($_).Count -eq 1 }).Count -eq 3) 'Each of the windows/amd64 host''s first three fast failures is one failed ... pid=- host=agentspace.windows-amd64 line or one started line for it, found before the backoff line that follows it' "Forms seen before each backoff line: $($forms -join ', '). The log: $($log -join ' | ')"
    Add-HostCheck (@(Get-HolderProcesses | Where-Object { ([string]$_.CommandLine).Contains((Get-HostAlias $pProject 'c2')) }).Count -eq 0) 'No holder is left running on the windows/amd64 host'
    $armStarted = @($log | Where-Object { $_ -cmatch ' started ' -and $_ -cmatch ' host=agentspace\z' })
    $recArm = Read-HostRecord $pProject
    Add-HostCheck ($armStarted.Count -eq 1 -and $null -ne $recArm -and (Test-HostLine $armStarted[0] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recArm['pid']) $pProject)) 'The linux/arm64 host''s holder is started once' "Started lines for it: $($armStarted.Count)."
    Add-HostCheck ($null -ne $recArm -and (Test-Alive ([int]$recArm['pid']))) 'The linux/arm64 host''s holder is still running' 'It is not.'
    $idx = @(0..([Math]::Max($log.Count, 1) - 1) | Where-Object { $_ -lt $log.Count -and $log[$_] -cmatch ' backoff seconds=\d+ host=agentspace\.windows-amd64\z' })
    $between = if ($idx.Count -ge 3 -and $null -ne $recArm) { @($log[($idx[1] + 1)..($idx[2] - 1)] | Where-Object { Test-HostLine $_ 'kept' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recArm['pid']) $pProject }) } else { @() }
    Add-HostCheck ($between.Count -ge 2) 'The linux/arm64 host''s kept lines go on every interval while the windows/amd64 host waits out its backoff: at least 2 between its second and third backoff line' "Kept lines between them: $($between.Count)."
  } }

  # H29a and H29b. A registry of the box, <p> and <q> in the ordinal order of their projects, whose records name no platform (a) or all name linux/arm64 (b): the keeper behaves as v0.11's did.
  foreach ($variant in @(@('H29a', $false), @('H29b', $true))) {
    if (Test-Selected $variant[0]) { Invoke-HostCase $variant[0] 'keeper.md#a-hosts-key-and-name' {
      $explicit = [bool]$variant[1]
      $label = if ($explicit) { 'Every record names linux/arm64' } else { 'No record names a platform' }
      $order = @($boxProject, $pProject, $qProject)
      Write-Registry @($order | ForEach-Object { New-PhRecord $_ $phArm 'ready' -Explicit:$explicit })
      $names = [ordered]@{ $boxProject = 'b1'; $pProject = 'c1'; $qProject = 'd1' }
      $leases = @($order | ForEach-Object { New-PhLease $_ $phArm $names[$_] '2026-11-17T13:00:00Z' })
      Write-Leases $leases
      $pass = Invoke-CapturedPass $now 3
      Add-HostCheck ($pass.Code -eq 0) "$label`: The first pass exits 0" "The exit code was $($pass.Code)."
      Add-HostCheck $pass.Complete "$label`: Three starts are made, each with its request, claim and result seen" "Starts seen: $(@($pass.Tokens).Count), in $($pass.Attempts) passes."
      $log = Read-LogLines
      $started = @(Select-ActionLines $log 'started')
      $lineOk = $log.Count -eq 3 -and $started.Count -eq 3
      for ($i = 0; $lineOk -and $i -lt 3; $i++) {
        $rec = Read-HostRecord $order[$i]
        if ($null -eq $rec -or -not (Test-HostLine $started[$i] 'started' (Get-HostLeaseId $names[$order[$i]]) (Get-HostAlias $order[$i] $names[$order[$i]]) ([string]$rec['pid']) $order[$i])) { $lineOk = $false }
      }
      Add-HostCheck $lineOk "$label`: Three started lines, ending in host=swimcoach, host=agentspace and host=zeta, in that order, each naming its own lease and holder" "Log: $($log -join ' | ')"
      $fieldsOk = @(Get-PhLineHosts $log).Count -eq 3 -and @(Get-PhLineHosts $log | Where-Object { $_ -cnotmatch $projectPattern }).Count -eq 0
      Add-HostCheck $fieldsOk "$label`: Every host= field is a project and nothing else" "Log: $($log -join ' | ')"
      $recordNames = @(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder.*\.json\z' } | Sort-Object)
      $recordsOk = ($recordNames -join ',') -ceq 'board-holder.agentspace.json,board-holder.json,board-holder.zeta.json'
      foreach ($recordName in $recordNames) { if (-not (Test-PhV011Record ([IO.File]::ReadAllText((Join-Path $env:AGENTSPACE_STATE_DIR $recordName))))) { $recordsOk = $false } }
      Add-HostCheck $recordsOk "$label`: The records are v0.11's three files, and none holds a platform member" "State folder: $((Get-StateNames) -join ', ')"
      Add-HostCheck ((Test-RequestLog (Get-CapturedRequest $pass 0) 'board-holder-ssh.log') -and (Test-RequestLog (Get-CapturedRequest $pass 1) 'board-holder-ssh.agentspace.log') -and (Test-RequestLog (Get-CapturedRequest $pass 2) 'board-holder-ssh.zeta.log')) "$label`: Each launch request's log is its project's: board-holder-ssh.log, board-holder-ssh.agentspace.log, board-holder-ssh.zeta.log" 'A request names another log, or one was not seen.'
      # A second pass keeps all three.
      $mark = $log.Count
      $before = @($recordNames | ForEach-Object { [IO.File]::ReadAllText((Join-Path $env:AGENTSPACE_STATE_DIR $_)) })
      $code = Invoke-Keeper $now
      $log = Read-LogLines
      $new = @(if ($log.Count -gt $mark) { $log[$mark..($log.Count - 1)] })
      $keptOk = $code -eq 0 -and $new.Count -eq 3
      for ($i = 0; $keptOk -and $i -lt 3; $i++) {
        $rec = Read-HostRecord $order[$i]
        if ($null -eq $rec -or -not (Test-HostLine $new[$i] 'kept' (Get-HostLeaseId $names[$order[$i]]) (Get-HostAlias $order[$i] $names[$order[$i]]) ([string]$rec['pid']) $order[$i])) { $keptOk = $false }
      }
      $after = @($recordNames | ForEach-Object { [IO.File]::ReadAllText((Join-Path $env:AGENTSPACE_STATE_DIR $_)) })
      Add-HostCheck ($keptOk -and ($after -join '|') -ceq ($before -join '|')) "$label`: A second pass keeps all three, in order, and changes no record" "Exit code $code. New lines: $($new -join ' | ')"
      # A third pass, with <p>'s lease released: <p>'s holder is stopped, the others kept.
      $holdPid = [int](Read-HostRecord $pProject)['pid']
      Write-Leases @((New-PhLease $boxProject $phArm 'b1' '2026-11-17T13:00:00Z'), (New-PhLease $pProject $phArm 'c1' '2026-11-17T13:00:00Z' -Released), (New-PhLease $qProject $phArm 'd1' '2026-11-17T13:00:00Z'))
      $mark = $log.Count
      $recBox = Read-HostRecord $boxProject
      $recQ = Read-HostRecord $qProject
      $code = Invoke-Keeper $now
      $log = Read-LogLines
      $new = @(if ($log.Count -gt $mark) { $log[$mark..($log.Count - 1)] })
      $stopOk = $code -eq 0 -and $new.Count -eq 3 -and $null -ne $recBox -and $null -ne $recQ -and
        (Test-HostLine $new[0] 'kept' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$recBox['pid']) $boxProject) -and
        (Test-HostLine $new[1] 'stopped' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$holdPid) $pProject) -and
        (Test-HostLine $new[2] 'kept' (Get-HostLeaseId 'd1') (Get-HostAlias $qProject 'd1') ([string]$recQ['pid']) $qProject)
      Add-HostCheck $stopOk "$label`: With <p>'s lease released, a pass logs kept host=swimcoach, stopped host=agentspace, kept host=zeta, in that order" "Exit code $code. New lines: $($new -join ' | ')"
      Add-HostCheck ((Wait-Until { -not (Test-Alive $holdPid) } 30) -and -not (Test-Path -LiteralPath (Get-HostRecordPath $pProject)) -and (Test-Path -LiteralPath (Get-HostRecordPath $qProject)) -and (Test-Path -LiteralPath (Get-HostRecordPath $boxProject))) "$label`: agentspace's holder is gone and its record removed, and the other two records stay" 'The holder still runs, the record is still there, or another record went.'
    } }
  }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $temp) -Recurse -Force -ErrorAction SilentlyContinue }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

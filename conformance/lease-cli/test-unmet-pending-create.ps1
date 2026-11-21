#Requires -Version 7
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which have no project list, no host registry,
# no `pending` and no provisioning run to start: a create answers `ready` there, and
# AGENTSPACE_PROVISION_CMD is read by nothing. A v0.07 build MUST pass every result line
# (docs/conformance.md, "The files that are expected to fail").
#
# Cases of E0.07-T04-S01 (card #0155), a create that answers `pending` and starts the provisioning run,
# each named by its case in docs/driver.md, "Conformance cases for v0.07". The ones that need a held lock
# for longer than 30 seconds, or a run that lasts a minute, are in the two files beside this one
# (test-unmet-pending-create-long.ps1: C6 and C29; test-unmet-pending-create-lock.ps1: C22).
#   C5   A listed project with no record: pending, no expires_at, ssh or reasons, no lease files, the record
#        creating with last_attempt_at within the run beside the box's record, and the stand-in started
#        once with exactly `provision -Host <project>`.
#   C7   The stand-in's working folder, standard input, standard output and standard error, and the four
#        variables of its environment that the detached start sets.
#   C8   A second create, or a get, within 5 minutes of last_attempt_at starts nothing and changes nothing.
#   C9   A record failed or creating whose last_attempt_at is 5 minutes or more old: a create starts the
#        stand-in once and sets state creating and last_attempt_at to now.
#   C10  With the project's provision.lock held, a create or a get starts nothing and writes nothing.
#   C16  A run that is due, with AGENTSPACE_PROVISION_CMD naming no file: internal, nothing recorded.
#   C18  A .cmd (and .CMD, and .bat) stand-in is started through cmd.exe /d /c, with the same arguments.
#   C23  A listed project with no record while provision.lock is held: pending, the record creating with no
#        last_attempt_at, nothing started.
#   C27  With no host folder before the create, the driver made hosts\<project>\ and started the stand-in.
#   C28  A variable only the CLI had, under a name of the check's own, is absent from the stand-in.
#   C30  A relative AGENTSPACE_PROVISION_CMD is made full against the CLI's working folder.
#   C31  With AGENTSPACE_PROVISION_CMD naming no file, a run that is due fails and one that is not due
#        answers pending, the registry as it was in each.
# Sections enforced: driver.md "Starting a provisioning run" (the program, steps 1 to 5, the detached
# start) and "Where it places a lease"; lease-api.md "States", "Environment" and "Messages"; state.md "The
# host registry", "A host's folder", "The provisioning run's lock" and "The provisioning run's start".
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check
# starts a real run: every create or get that could start one is given AGENTSPACE_PROVISION_CMD naming a
# stand-in under the home that records what it is given and makes no host. A stand-in still running when
# its check ends is stopped by the PID it recorded, only if it still has the name and start time it
# recorded. Every write calls Assert-ConformancePath at the call, the stand-in's own writes beside itself
# excepted. A failure reason never quotes a value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-pending-create.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 3 minutes.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$start = 'driver.md#starting-a-provisioning-run'

# ---- C5 -------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C5: A create for a listed project with no record answers pending, adds the record creating with last_attempt_at, and starts the stand-in once with provision -Host <project>" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $before = Get-CcNowSecond
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  $after = Get-CcNowSecond
  $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  foreach ($x in (Get-PvHostsFileProblems -Lab $lab)) { $p.Add($x) }
  $rec = Get-PvHostRecord $lab 'agentspace'
  if ($null -eq $rec) { $p.Add('hosts.json holds no record for the project') }
  else {
    $time = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $time -Before $before -After $after)) { $p.Add('last_attempt_at is not within the command''s run') }
    $want = New-PvHostsText -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt $time))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add($x) }
  }
  if ($run.Json -is [hashtable]) {
    $lease = Get-CcRecord $lab ([string]$run.Json['id'])
    if ($null -eq $lease) { $p.Add('The lease is not in the lease state file') }
    else {
      if ($lease['state'] -cne 'pending') { $p.Add('The stored lease is not pending') }
      if ([string]::IsNullOrEmpty([string]$lease['driver'])) { $p.Add('The stored lease carries no driver') }
    }
    if (-not (Test-CcStateFile $lab)) { $p.Add('The lease state file does not validate against its schema') }
  }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  return , $p
}

# ---- C7 -------------------------------------------------------------------------------------------------
function Test-RunEnvironment {
  param([string]$Label, [bool]$VariablesSet)
  $sub = @{ Name = "$start C7: $Label"; Set = $VariablesSet }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    Set-PvProjects -Lab $lab -Names @('agentspace')
    $work = Join-Path $lab.UserProfile 'work'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $work) | Out-Null
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    [Environment]::CurrentDirectory = (Assert-ConformancePath -ConformanceHome $lab -Path $work)
    $proxy = 'agentspace-conformance-proxy --not-run "two words" 100%'
    if ($sub.Set) { $vars = @{ AGENTSPACE_STATE_DIR = 'rel-state'; AGENTSPACE_SSH_DIR = 'rel-ssh'; AGENTSPACE_PROXY_COMMAND = $proxy } }
    else { $vars = @{ AGENTSPACE_STATE_DIR = ''; AGENTSPACE_SSH_DIR = ''; AGENTSPACE_PROXY_COMMAND = $null } }
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn -EnvVars $vars -InputJson 'Text that the stand-in MUST NOT receive.'
    $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
    if ($run.Stdout.Contains('STANDIN-') -or $run.Stderr.Contains('STANDIN-')) { $p.Add('Something the stand-in wrote reached the CLI''s standard output or standard error') }
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
    $calls = @(Get-PvCalls $standIn)
    if ($calls.Count -ge 1) {
      $c = $calls[0]
      $names = @($c['var_names'])
      if ($names.Count -lt 2 -or $names -notcontains 'USERPROFILE') { $p.Add('The stand-in recorded no environment') }
      if ((ConvertTo-PvFullPath $c['cwd']) -ine (ConvertTo-PvFullPath $lab.UserProfile)) { $p.Add('The stand-in''s working folder is not the home folder') }
      if ($c['stdin_redirected'] -and [int]$c['stdin_bytes'] -gt 0) { $p.Add("The stand-in's standard input held $($c['stdin_bytes']) bytes") }
      if ((ConvertTo-PvFullPath $c['userprofile']) -ine (ConvertTo-PvFullPath $lab.UserProfile)) { $p.Add('USERPROFILE is not the CLI''s') }
      if ($sub.Set) {
        if ((ConvertTo-PvFullPath $c['state_dir']) -ine (ConvertTo-PvFullPath (Join-Path $work 'rel-state'))) { $p.Add('AGENTSPACE_STATE_DIR is not the CLI''s, made full against its working folder') }
        if ((ConvertTo-PvFullPath $c['ssh_dir']) -ine (ConvertTo-PvFullPath (Join-Path $work 'rel-ssh'))) { $p.Add('AGENTSPACE_SSH_DIR is not the CLI''s, made full against its working folder') }
        if ($c['proxy_command'] -cne $proxy) { $p.Add('AGENTSPACE_PROXY_COMMAND is not the CLI''s exactly') }
      } else {
        foreach ($n in 'AGENTSPACE_STATE_DIR', 'AGENTSPACE_SSH_DIR', 'AGENTSPACE_PROXY_COMMAND') { if ($names -contains $n) { $p.Add("$n is present, and the CLI had it unset or empty") } }
      }
    }
    return , $p
  }
}
Test-RunEnvironment -Label 'The stand-in works in the home folder, reads nothing from standard input, writes nothing to the CLI''s streams, and has USERPROFILE and the three variables the CLI had, the two folders made full and the ProxyCommand exactly' -VariablesSet $true
Test-RunEnvironment -Label 'With AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR empty and AGENTSPACE_PROXY_COMMAND unset in the CLI, each is absent from the stand-in' -VariablesSet $false

# ---- C8 -------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C8: After a create that started the run, a second create and a get of the project within 5 minutes answer pending, start nothing and leave last_attempt_at as it was" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $first = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  $p = Get-PvPendingDocProblems -Run $first -Project 'agentspace'
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  $time = [string](Get-PvHostRecord $lab 'agentspace')['last_attempt_at']
  $hash = Get-PvHostsHash $lab
  $second = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  foreach ($x in (Get-PvPendingDocProblems -Run $second -Project 'agentspace')) { $p.Add("second create: $x") }
  if ($first.Json -is [hashtable]) {
    $get = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$first.Json['id']) -StandIn $standIn
    foreach ($x in (Get-PvPendingDocProblems -Run $get -Project 'agentspace')) { $p.Add("get: $x") }
  }
  Start-Sleep -Seconds 3
  $n = @(Get-PvStartedLines $standIn).Count
  if ($n -ne 1) { $p.Add("The stand-in was started $n times in all, not once") }
  if ([string](Get-PvHostRecord $lab 'agentspace')['last_attempt_at'] -cne $time) { $p.Add('last_attempt_at changed') }
  if ((Get-PvHostsHash $lab) -ne $hash) { $p.Add('hosts.json was written') }
  return , $p
}
foreach ($state in 'creating', 'failed') {
  $sub = @{ State = $state }
  Use-PvLab -Name "$start C8: A record $state whose last_attempt_at is 4 minutes 50 seconds old: a create and then a get answer pending and start nothing, with hosts.json as it was" -Body {
    param($lab)
    Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State $sub.State -LastAttemptAt (Get-PvAgo 290)))
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $hash = Get-PvHostsHash $lab
    $create = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
    $p = Get-PvPendingDocProblems -Run $create -Project 'agentspace'
    if ($create.Json -is [hashtable]) {
      $get = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$create.Json['id']) -StandIn $standIn
      foreach ($x in (Get-PvPendingDocProblems -Run $get -Project 'agentspace')) { $p.Add("get: $x") }
    }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    if ((Get-PvHostsHash $lab) -ne $hash) { $p.Add('hosts.json was written') }
    return , $p
  }
}

# ---- C9 -------------------------------------------------------------------------------------------------
function Test-RunDue {
  param([string]$Label, [string]$State, [string]$AgeSeconds, [string]$InstanceId)
  $sub = @{ Name = "$start C9: $Label"; State = $State; Age = $AgeSeconds; Instance = $InstanceId }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $old = $(if ($sub.Age) { Get-PvAgo ([int]$sub.Age) } else { '' })
    Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State $sub.State -LastAttemptAt $old -InstanceId $sub.Instance))
    $standIn = New-PvStandIn -Lab $lab -Name 'run'
    $before = Get-CcNowSecond
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
    $after = Get-CcNowSecond
    $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
    $rec = Get-PvHostRecord $lab 'agentspace'
    if ($null -eq $rec) { $p.Add('The record is gone from hosts.json') }
    else {
      $time = [string]$rec['last_attempt_at']
      if (-not (Test-PvTimeBetween -Time $time -Before $before -After $after)) { $p.Add('last_attempt_at is not now') }
      $want = New-PvHostsText -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt $time -InstanceId $sub.Instance))
      foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add($x) }
    }
    return , $p
  }
}
Test-RunDue -Label 'A record failed, last attempt 6 minutes ago, with an instance_id: a create starts the stand-in once and sets creating and last_attempt_at to now, keeping the rest' -State 'failed' -AgeSeconds '360' -InstanceId 'i-1234567890abcdef0'
Test-RunDue -Label 'A record creating, last attempt exactly 5 minutes ago (the lower bound): a create starts the stand-in once' -State 'creating' -AgeSeconds '300' -InstanceId ''
Test-RunDue -Label 'A record creating, last attempt 15 minutes ago, whose run died: a create starts the stand-in once' -State 'creating' -AgeSeconds '900' -InstanceId ''
Test-RunDue -Label 'A record failed with no last_attempt_at: a create starts the stand-in once' -State 'failed' -AgeSeconds '' -InstanceId ''

# ---- C10 ------------------------------------------------------------------------------------------------
function Test-LockHeld {
  param([string]$Label, [string]$State, [string]$AgeSeconds)
  $sub = @{ Name = "$start C10: $Label"; State = $State; Age = $AgeSeconds }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    $old = $(if ($sub.Age) { Get-PvAgo ([int]$sub.Age) } else { '' })
    Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State $sub.State -LastAttemptAt $old))
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    [void](Open-PvLock -Lab $lab -Path (Get-PvProvisionLockPath $lab 'agentspace'))
    $hash = Get-PvHostsHash $lab
    $create = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
    $p = Get-PvPendingDocProblems -Run $create -Project 'agentspace'
    if ($create.Json -is [hashtable]) {
      $get = Invoke-PvLease -Lab $lab -Arguments @('get', [string]$create.Json['id']) -StandIn $standIn
      foreach ($x in (Get-PvPendingDocProblems -Run $get -Project 'agentspace')) { $p.Add("get: $x") }
    }
    foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
    if ((Get-PvHostsHash $lab) -ne $hash) { $p.Add('hosts.json was written: the record''s state or last_attempt_at changed') }
    return , $p
  }
}
Test-LockHeld -Label 'With provision.lock held, a create and a get for a record creating whose last attempt is 15 minutes old start nothing and leave hosts.json as it was' -State 'creating' -AgeSeconds '900'
Test-LockHeld -Label 'With provision.lock held, a create and a get for a record failed with no last_attempt_at start nothing and leave hosts.json as it was' -State 'failed' -AgeSeconds ''

# ---- C16 and C31 ----------------------------------------------------------------------------------------
function Test-NoProgram {
  param([string]$Case, [string]$Label, [scriptblock]$Seed, [string]$Program, [string]$Expect)
  $sub = @{ Name = "$start $Case`: $Label"; Seed = $Seed; Program = $Program; Expect = $Expect }
  Use-PvLab -Name $sub.Name -Body {
    param($lab)
    & $sub.Seed $lab
    $folder = Join-Path $lab.Root 'standins'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $folder) | Out-Null
    $path = $(if ($sub.Program -ceq 'folder') { $folder } else { Join-Path $folder 'no-such-program.cmd' })
    $hash = Get-PvHostsHash $lab
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -EnvVars @{ AGENTSPACE_PROVISION_CMD = $path }
    if ($sub.Expect -ceq 'internal') {
      $p = Get-PvInternalProblems -Run $run
      foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
    } else {
      $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
    }
    if ((Get-PvHostsHash $lab) -ne $hash) { $p.Add('hosts.json was written') }
    return , $p
  }
}
$listed = { param($lab) Set-PvProjects -Lab $lab -Names @('agentspace') }
$dueRecord = { param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt (Get-PvAgo 900))) }
$notDueRecord = { param($lab) Set-PvHosts -Lab $lab -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt (Get-PvAgo 60))) }
Test-NoProgram -Case 'C16' -Label 'A create for a listed project with no record, whose run is due, with AGENTSPACE_PROVISION_CMD naming no file, fails with internal, records nothing, writes no registry and says no path' -Seed $listed -Program 'missing' -Expect 'internal'
Test-NoProgram -Case 'C16' -Label 'A create whose record is creating and whose run is due, with AGENTSPACE_PROVISION_CMD naming no file, fails with internal and leaves hosts.json as it was' -Seed $dueRecord -Program 'missing' -Expect 'internal'
Test-NoProgram -Case 'C16' -Label 'AGENTSPACE_PROVISION_CMD naming a folder is a program that cannot be found: a create whose run is due fails with internal' -Seed $dueRecord -Program 'folder' -Expect 'internal'
Test-NoProgram -Case 'C31' -Label 'With AGENTSPACE_PROVISION_CMD naming no file and a run that is due, create fails with internal and leaves the registry as it was' -Seed $dueRecord -Program 'missing' -Expect 'internal'
Test-NoProgram -Case 'C31' -Label 'With AGENTSPACE_PROVISION_CMD naming no file and no run due (last attempt 1 minute ago), the same create answers pending and leaves the registry as it was' -Seed $notDueRecord -Program 'missing' -Expect 'pending'

# ---- C18 ------------------------------------------------------------------------------------------------
# The program's quoting in the command line is illustrative (architect, 2026-11-21, driver.md "Starting a
# provisioning run"): the detached start quotes an argument only when it is empty or holds a space, a tab or
# a non-printable character. So the check accepts the program quoted or not, and requires the quotes only for
# a path that holds a space, where the stand-in MUST still be started with exactly the three arguments.
function Test-CmdStandIn {
  param([string]$Extension, [string]$FolderName = 'run')
  $sub = @{ Ext = $Extension; Folder = $FolderName }
  $what = $(if ($FolderName.Contains(' ')) { "A .$Extension stand-in whose full path holds a space is started through the system folder's cmd.exe /d /c, with the program quoted, and receives provision -Host <project>" } else { "A .$Extension stand-in is started through the system folder's cmd.exe /d /c and receives provision -Host <project>" })
  Use-PvLab -Name "$start C18: $what" -Body {
    param($lab)
    Set-PvProjects -Lab $lab -Names @('agentspace')
    $standIn = New-PvStandIn -Lab $lab -Name $sub.Folder -Extension $sub.Ext
    $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
    $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
    foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
    $calls = @(Get-PvCalls $standIn)
    if ($calls.Count -ge 1) {
      $c = $calls[0]
      if ([string]$c['parent_name'] -cne 'cmd.exe') { $p.Add('The stand-in''s parent is not cmd.exe') }
      $m = [regex]::Match([string]$c['parent_command_line'], '\A(?<cmd>.+?) /d /c (?<prog>"[^"]+"|\S+) provision -Host agentspace\z')
      if (-not $m.Success) { $p.Add('The interpreter''s command line is not <system folder>\cmd.exe /d /c <program> provision -Host <project>') }
      else {
        $sys = $env:SystemRoot.TrimEnd('\', '/') + '\System32\cmd.exe'
        if ((ConvertTo-PvFullPath $m.Groups['cmd'].Value.Trim('"')) -ine (ConvertTo-PvFullPath $sys)) { $p.Add('The interpreter is not the system folder''s cmd.exe') }
        if ((ConvertTo-PvFullPath $m.Groups['prog'].Value.Trim('"')) -ine (ConvertTo-PvFullPath $standIn.Path)) { $p.Add('The program in the command line is not the one AGENTSPACE_PROVISION_CMD names') }
        if ($standIn.Path.Contains(' ') -and -not ($m.Groups['prog'].Value.StartsWith('"') -and $m.Groups['prog'].Value.EndsWith('"'))) { $p.Add('The program''s path holds a space and is not in double quotes') }
      }
    }
    return , $p
  }
}
Test-CmdStandIn -Extension 'cmd'
Test-CmdStandIn -Extension 'CMD'
Test-CmdStandIn -Extension 'bat'
Test-CmdStandIn -Extension 'cmd' -FolderName 'run with space'


# ---- C23 ------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C23: A create for a listed project with no record, while provision.lock is held, answers pending, adds the record creating with no last_attempt_at and starts nothing" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  [void](Open-PvLock -Lab $lab -Path (Get-PvProvisionLockPath $lab 'agentspace'))
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
  $want = New-PvHostsText -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating'))
  foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add($x) }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

# ---- C27 ------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C27: With no host folder before the create, the driver made hosts\<project>\, never made provision.lock, and the stand-in was started" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $p = [System.Collections.Generic.List[string]]::new()
  $folder = Get-PvHostFolder $lab 'agentspace'
  if (Test-Path -LiteralPath (Join-Path $lab.UserProfile '.agentspace\hosts')) { $p.Add('There was a hosts folder before the create') }
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  foreach ($x in (Get-PvPendingDocProblems -Run $run -Project 'agentspace')) { $p.Add($x) }
  if (-not (Test-Path -LiteralPath $folder -PathType Container)) { $p.Add('The driver did not make hosts\<project>\') }
  if (Test-Path -LiteralPath (Get-PvProvisionLockPath $lab 'agentspace')) { $p.Add('There is a provision.lock: only a run makes it') }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  return , $p
}

# ---- C28 ------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C28: Variables set only in the CLI's environment, under names of the check's own, are absent from the stand-in's environment" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $tag = [guid]::NewGuid().ToString('N').Substring(0, 12).ToUpperInvariant()
  $probeA = "AGENTSPACE_CONFORMANCE_PROBE_$tag"
  $probeB = "CONFORMANCE_PROBE_$tag"
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn -EnvVars @{ $probeA = 'present'; $probeB = 'present' }
  $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  $calls = @(Get-PvCalls $standIn)
  if ($calls.Count -ge 1) {
    $names = @($calls[0]['var_names'])
    if ($names.Count -lt 2 -or $names -notcontains 'USERPROFILE') { $p.Add('The stand-in recorded no environment, so the absence below shows nothing') }
    if ($names -contains $probeA) { $p.Add('A variable of the CLI''s whose name starts AGENTSPACE_ reached the stand-in') }
    if ($names -contains $probeB) { $p.Add('A variable of the CLI''s reached the stand-in') }
  }
  return , $p
}

# ---- C30 ------------------------------------------------------------------------------------------------
Use-PvLab -Name "$start C30: A relative AGENTSPACE_PROVISION_CMD, with the CLI's working folder other than the home folder, is made full against that working folder and the stand-in it names is started" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $work = Join-Path $lab.UserProfile 'work'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $work) | Out-Null
  [Environment]::CurrentDirectory = (Assert-ConformancePath -ConformanceHome $lab -Path $work)
  $relative = '..\..\standins\run\' + (Split-Path -Leaf $standIn.Path)
  if ((ConvertTo-PvFullPath (Join-Path $work $relative)) -ine (ConvertTo-PvFullPath $standIn.Path)) { throw 'The relative path does not reach the stand-in' }
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -EnvVars @{ AGENTSPACE_PROVISION_CMD = $relative }
  $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  return , $p
}

Exit-ConformanceRun

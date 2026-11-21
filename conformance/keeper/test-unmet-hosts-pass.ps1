#Requires -Version 7
# A conformance check under D34: it drives the keeper only as a separate process, through what docs/keeper.md and docs/state.md document, with a stand-in for ssh (a program outside agentspace). See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# THE ONE-PASS CASES OF A HOST PER PROJECT (v0.07, docs/keeper.md, "Hosts", "The hosts of a pass", and
# "Conformance cases for a host per project", cases H2 to H8, H12 to H14, H16 and H18, written by
# E0.07-T04-S03, card #0157), and the card's own cases: each host's holder writes its own
# board-holder-ssh.<project>.log, and a registry that cannot be read gives its two reasons. It
# checks behavior that v0.06's keeper, which knows one holder, does not have, so it is expected to FAIL
# against v0.06's keeper in most of its cases (H2, H8 and H18's no-error check may pass there, which is
# right); the rewrite's v0.07 keeper MUST pass every line. The cases that need a keeper that watches
# are in test-unmet-hosts-watch.ps1.
#
# Each case runs against the program that AGENTSPACE_KEEPER_CMD names (or its default, through the
# harness) with the keeper's own arguments unchanged: <program> -SshExe <stub> -Now <time>. Each runs
# with a throwaway USERPROFILE and state folder, a stub -SshExe and a hosts.json the case writes under
# the throwaway USERPROFILE (never the state folder), or none. <p> is agentspace, a project with a
# ready record of its own; the box's project is swimcoach. No case needs a real host: a holder's
# connection is a stub's.
#
# What it touches: the same as test-keeper.ps1 (conformance/keeper/keeper-common.ps1, and this
# suite's conformance/keeper/keeper-hosts-common.ps1): a throwaway folder per case under $env:TEMP, the
# harness's child environment for it on every process, the stand-in ssh-hosts-stub.cmd (written by
# the run into its own folder), and only processes this run started, each stopped by a checked PID. The
# real %USERPROFILE%\.agentspace, ~/.ssh, ssh.exe, keeper task and box are never read, written,
# listed or hashed. Every write and delete goes through Assert-ConformancePath at the call.
#
# Every wait is on what a case checks for, with a deadline, never a fixed sleep (keeper.md, "Checking the
# keeper", card #0066).
#
# Output: PASS <check name>, FAIL <check name>: <reason>, SKIPPED <check name>: <reason>, then one
# summary line; exit 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.
# Run:  pwsh -NoProfile -File conformance\keeper\test-unmet-hosts-pass.ps1
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
  Write-ConformanceResult -Name 'keeper.md#command-line: The program that AGENTSPACE_KEEPER_CMD names exists' -Status FAIL -Reason $(if ($keeperCmd) { "$keeperCmd is not a file." } else { 'Neither the variable nor its default names a program.' })
  Exit-ConformanceRun
}

Initialize-KeeperRun
Initialize-HostsRun

# A case whose id is not matched by -Case is not run.
function Test-Selected([string]$Id) { $Id -like $Case }

try {
  # H2. No registry; a ready lease of any project: the holder is the box's.
  if (Test-Selected 'H2') { Invoke-HostCase 'H2' 'keeper.md#hosts' {
    Write-Leases @((New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    $r = Invoke-CapturedPass $now 1
    Add-HostCheck ($r.Code -eq 0) 'With no registry the pass exits 0' "The exit code was $($r.Code)."
    Add-HostCheck $r.Complete 'One start is made, and its launch files are seen' "Starts seen: $(@($r.Tokens).Count) in $($r.Attempts) passes."
    $rec = Read-HostRecord $boxProject
    Add-HostCheck ($null -ne $rec -and $rec['lease_id'] -ceq (Get-HostLeaseId 'c1') -and $rec['alias'] -ceq (Get-HostAlias $pProject 'c1')) 'board-holder.json is written, naming the lease and its alias' 'The box record is absent or names another lease.'
    Add-HostCheck ((Get-OtherRecordNames).Count -eq 0) 'No board-holder.<project>.json appears' "Found: $((Get-OtherRecordNames) -join ', ')"
    Add-HostCheck (Test-RequestLog (Get-CapturedRequest $r 0) 'board-holder-ssh.log') 'The launch request''s log is board-holder-ssh.log' 'The request names another log, or none was seen.'
    Add-HostCheck (@(Get-StateNames | Where-Object { $_ -cmatch '\Aboard-holder-ssh\.[a-z0-9-]+\.log' }).Count -eq 0) 'No board-holder-ssh.<project>.log appears' 'A per-project holder log was found.'
    $log = Read-LogLines
    $st = @(Select-ActionLines $log 'started')
    Add-HostCheck ($log.Count -eq 1 -and $st.Count -eq 1 -and $null -ne $rec -and (Test-HostLine $st[0] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$rec['pid']) $boxProject)) 'One started line names the lease and ends in host=swimcoach'
  } }

  # H3 and the card's per-host holder log. Two hosts, both ready: one pass starts two holders, the box's first.
  if (Test-Selected 'H3') { Invoke-HostCase 'H3' 'keeper.md#the-order-of-a-pass' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T14:00:00Z'))
    $r = Invoke-CapturedPass $now 2
    Add-HostCheck ($r.Code -eq 0) 'The pass exits 0' "The exit code was $($r.Code)."
    Add-HostCheck $r.Complete 'Two starts are made, each with its request, claim and result seen' "Starts seen: $(@($r.Tokens).Count), complete sets: see the attempts ($($r.Attempts))."
    $recB = Read-HostRecord $boxProject
    $recP = Read-HostRecord $pProject
    $log = Read-LogLines
    $st = @(Select-ActionLines $log 'started')
    Add-HostCheck ($log.Count -eq 2 -and $st.Count -eq 2) 'Two started lines are logged, and nothing else' "Log: $($log -join ' | ')"
    Add-HostCheck ($st.Count -ge 1 -and $null -ne $recB -and (Test-HostLine $st[0] 'started' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$recB['pid']) $boxProject)) 'The first started line is the box''s, ending in host=swimcoach'
    Add-HostCheck ($st.Count -ge 2 -and $null -ne $recP -and (Test-HostLine $st[1] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recP['pid']) $pProject)) 'The second started line is agentspace''s, ending in host=agentspace'
    Add-HostCheck ($null -ne $recB -and $null -ne $recP -and [int]$recB['pid'] -ne [int]$recP['pid'] -and (Test-Alive ([int]$recB['pid'])) -and (Test-Alive ([int]$recP['pid']))) 'Board-holder.json and board-holder.agentspace.json name two different running holders' 'A record is absent, or a holder is not running.'
    Add-HostCheck (Test-RequestLog (Get-CapturedRequest $r 0) 'board-holder-ssh.log') 'The first start''s launch request names board-holder-ssh.log' 'The first request names another log.'
    Add-HostCheck (Test-RequestLog (Get-CapturedRequest $r 1) 'board-holder-ssh.agentspace.log') 'The second start''s launch request names board-holder-ssh.agentspace.log' 'The second request names another log.'
    Add-HostCheck ($r.MaxConcurrent -le 1) 'The two starts'' launch files never exist at the same time: the second request appears only after the first start''s files are gone' "Two starts' files were in the folder together: $($r.Overlap)"
    Add-HostCheck (@(Get-StateNames | Where-Object { $_ -like 'board-holder-launch.*' }).Count -eq 0) 'No launch file is left after the pass'
  } }

  # The card's case: each host's holder writes its own board-holder-ssh.<project>.log (the launch request's log).
  if (Test-Selected 'X1') { Invoke-HostCase 'X1' 'keeper.md#the-holders-log' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T14:00:00Z'))
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $aliasB = Get-HostAlias $boxProject 'b1'
    $aliasP = Get-HostAlias $pProject 'c1'
    $gotB = Wait-Until { $t = Get-HostSshLogText $boxProject; $t -and $t.Contains("stub-started") -and $t.Contains($aliasB) } 30
    $gotP = Wait-Until { $t = Get-HostSshLogText $pProject; $t -and $t.Contains("stub-started") -and $t.Contains($aliasP) } 30
    Add-HostCheck $gotB 'The box''s holder writes its output to board-holder-ssh.log' 'board-holder-ssh.log never held the box holder''s line.'
    Add-HostCheck $gotP 'agentspace''s holder writes its output to board-holder-ssh.agentspace.log' 'board-holder-ssh.agentspace.log never held agentspace''s holder''s line.'
    $tb = [string](Get-HostSshLogText $boxProject)
    $tp = [string](Get-HostSshLogText $pProject)
    Add-HostCheck (-not $tb.Contains($aliasP) -and -not $tp.Contains($aliasB)) 'Neither log holds the other host''s holder''s output' 'A holder wrote to the other host''s log.'
  } }

  # H4. Two ready leases on each of two hosts: each holder goes through its own host's latest-expiring lease, ties to the lowest ID.
  if (Test-Selected 'H4') { Invoke-HostCase 'H4' 'keeper.md#which-lease' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    # The box's two differ in expiry, the later one second in the file. agentspace's two share an expiry, the higher ID first in the file,
    # and expire before the box's later one, so a keeper that pools every lease would choose the box's.
    Write-Leases @(
      (New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $boxProject 'b2' '2026-11-17T15:00:00Z'),
      (New-HostLease $pProject 'c2' '2026-11-17T14:30:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T14:30:00Z'))
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $recB = Read-HostRecord $boxProject
    $recP = Read-HostRecord $pProject
    Add-HostCheck ($null -ne $recB -and $recB['lease_id'] -ceq (Get-HostLeaseId 'b2') -and $recB['alias'] -ceq (Get-HostAlias $boxProject 'b2')) 'The box''s holder goes through the box lease that expires last' 'The box record is absent or names another lease.'
    Add-HostCheck ($null -ne $recP -and $recP['lease_id'] -ceq (Get-HostLeaseId 'c1') -and $recP['alias'] -ceq (Get-HostAlias $pProject 'c1')) 'agentspace''s holder goes through its lease with the lowest ID among those that share the latest expiry' 'The agentspace record is absent or names another lease.'
    $clB = if ($recB) { Get-CommandLine ([int]$recB['pid']) } else { '' }
    $clP = if ($recP) { Get-CommandLine ([int]$recP['pid']) } else { '' }
    Add-HostCheck ($clB.Contains((Get-HostAlias $boxProject 'b2')) -and -not $clB.Contains('-agentspace-') -and $clP.Contains((Get-HostAlias $pProject 'c1')) -and -not $clP.Contains((Get-HostAlias $pProject 'c2')) -and -not $clP.Contains('-swimcoach-')) 'Each holder runs for its own host''s alias, and never the other host''s' "Box holder: $clB; agentspace holder: $clP"
    Add-HostCheck ((Get-HolderProcesses).Count -eq 2) 'Two holders only' "Holders found: $((Get-HolderProcesses).Count)"
  } }

  # H5. A ready <p> lease while <p>'s record is creating, failed, then absent: the lease is on the box each time.
  if (Test-Selected 'H5') { Invoke-HostCase 'H5' 'keeper.md#which-host-a-lease-is-on' {
    Write-Leases @((New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    foreach ($state in 'creating', 'failed', 'absent') {
      Reset-HostsState
      if ($state -eq 'absent') { Write-Registry @((New-HostRecord $boxProject)) } else { Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject $state)) }
      $code = Invoke-Keeper $now
      Add-HostCheck ($code -eq 0) "Record $state`: The pass exits 0" "The exit code was $code."
      $recB = Read-HostRecord $boxProject
      Add-HostCheck ($null -ne $recB -and $recB['lease_id'] -ceq (Get-HostLeaseId 'c1') -and $recB['alias'] -ceq (Get-HostAlias $pProject 'c1')) "Record $state`: The box's holder goes through the lease" 'The box record is absent or names another lease.'
      Add-HostCheck ((Get-OtherRecordNames).Count -eq 0) "Record $state`: No board-holder.<project>.json is written" "Found: $((Get-OtherRecordNames) -join ', ')"
      $log = Read-LogLines
      $st = @(Select-ActionLines $log 'started')
      Add-HostCheck ($log.Count -eq 1 -and $st.Count -eq 1 -and $null -ne $recB -and (Test-HostLine $st[0] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recB['pid']) $boxProject)) "Record $state`: One started line ends in host=swimcoach"
    }
  } }

  # H6. The hand-edit safeguard: <p>'s record is rewritten to creating while its holder runs.
  if (Test-Selected 'H6') { Invoke-HostCase 'H6' 'keeper.md#stopping-a-holder' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    $hb = Start-HostHolder $boxProject 'b1'
    $hp = Start-HostHolder $pProject 'c1'
    $recordB = Get-HostRecordPath $boxProject
    $before = [IO.File]::ReadAllText($recordB)
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject 'creating'))
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 2 -and (Test-HostLine $log[0] 'kept' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$hb.Pid) $boxProject)) 'The box''s turn logs kept, host=swimcoach, first' "Log: $($log -join ' | ')"
    Add-HostCheck ($log.Count -eq 2 -and (Test-HostLine $log[1] 'stopped' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$hp.Pid) $pProject)) 'agentspace''s turn logs stopped, host=agentspace, and no holder is started' "Log: $($log -join ' | ')"
    $gone = Wait-Until { -not (Test-Alive $hp.Pid) } 30
    Add-HostCheck $gone 'agentspace''s holder process is gone' 'The stub is still running.'
    Add-HostCheck (-not (Test-Path -LiteralPath (Get-HostRecordPath $pProject))) 'board-holder.agentspace.json is removed' 'The record is still there.'
    Add-HostCheck ((Test-TreeAlive $hb) -and (Test-Path -LiteralPath $recordB) -and [IO.File]::ReadAllText($recordB) -ceq $before) 'The box''s holder is still running and its record is unchanged' 'The box holder was stopped or its record changed.'
  } }

  # H7. A holder recorded for a project with no record in the registry gets a turn and is stopped.
  if (Test-Selected 'H7') { Invoke-HostCase 'H7' 'keeper.md#the-hosts-of-a-pass' {
    Write-Registry @((New-HostRecord $boxProject))
    Write-Leases @()
    $hq = Start-HostHolder $qProject 'd1'
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 2 -and (Test-IdleLine $log[0])) 'The box''s turn comes first and logs idle with no host' "Log: $($log -join ' | ')"
    Add-HostCheck ($log.Count -eq 2 -and (Test-HostLine $log[1] 'stopped' (Get-HostLeaseId 'd1') (Get-HostAlias $qProject 'd1') ([string]$hq.Pid) $qProject)) 'The turn of the project with no record stops its holder: stopped, host=zeta' "Log: $($log -join ' | ')"
    Add-HostCheck (Wait-Until { -not (Test-Alive $hq.Pid) } 30) 'The holder process is gone' 'The stub is still running.'
    Add-HostCheck (-not (Test-Path -LiteralPath (Get-HostRecordPath $qProject))) 'board-holder.zeta.json is removed' 'The record is still there.'
  } }

  # H8. A board-holder.swimcoach.json is never a record: never read, removed or stopped.
  if (Test-Selected 'H8') { Invoke-HostCase 'H8' 'keeper.md#the-hosts-of-a-pass' {
    $fileName = 'board-holder.swimcoach.json'
    $stray = Get-HostRecordPath $boxProject $fileName
    # Part A: a ready box lease and no board-holder.json. A keeper that read the stray file as the box's record would keep its holder, and start none.
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'))
    $h = Start-Stub (Get-HostAlias $boxProject 'b1')
    Write-HostRecord $boxProject $h.Pid $h.Started (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') $fileName
    $before = [IO.File]::ReadAllText($stray)
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'With a ready lease: The pass exits 0' "The exit code was $code."
    $recB = Read-HostRecord $boxProject
    $log = Read-LogLines
    $st = @(Select-ActionLines $log 'started')
    Add-HostCheck ($log.Count -eq 1 -and $st.Count -eq 1 -and $null -ne $recB -and [int]$recB['pid'] -ne $h.Pid -and (Test-HostLine $st[0] 'started' (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') ([string]$recB['pid']) $boxProject)) 'With a ready lease: The box''s turn uses board-holder.json alone, so it starts its own holder' "Log: $($log -join ' | ')"
    Add-HostCheck ((Test-TreeAlive $h) -and (Test-Path -LiteralPath $stray) -and [IO.File]::ReadAllText($stray) -ceq $before) 'With a ready lease: The stray file is not removed or changed, and its process is left running' 'The stray file or its process was touched.'
    # Part B: no ready lease. A keeper that read the stray file would stop the holder it names.
    Reset-HostsState
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z' -Released))
    $h = Start-Stub (Get-HostAlias $boxProject 'b1')
    Write-HostRecord $boxProject $h.Pid $h.Started (Get-HostLeaseId 'b1') (Get-HostAlias $boxProject 'b1') $fileName
    $before = [IO.File]::ReadAllText($stray)
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'With no ready lease: The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 1 -and (Test-IdleLine $log[0])) 'With no ready lease: One idle line is logged, with no host and no stopped line' "Log: $($log -join ' | ')"
    Add-HostCheck ((Test-TreeAlive $h) -and (Test-Path -LiteralPath $stray) -and [IO.File]::ReadAllText($stray) -ceq $before) 'With no ready lease: The stray file is not removed or changed, and its process is left running' 'The stray file or its process was touched.'
  } }

  # H12 and the card's two reasons. A registry that cannot be read fails the pass; both holders and both records stay as they are.
  if (Test-Selected 'H12') { Invoke-HostCase 'H12' 'keeper.md#hosts' {
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    $hb = Start-HostHolder $boxProject 'b1'
    $hp = Start-HostHolder $pProject 'c1'
    $textB = [IO.File]::ReadAllText((Get-HostRecordPath $boxProject))
    $textP = [IO.File]::ReadAllText((Get-HostRecordPath $pProject))
    $good = @{ Box = (New-HostRecord $boxProject); P = (New-HostRecord $pProject) }
    $invalid = 'the host registry is not valid'
    $variants = @(
      @{ Name = 'a file that is not JSON'; Error = $invalid; Make = { Write-RegistryText '{ this is not json' } },
      @{ Name = 'a file that is not UTF-8'; Error = $invalid; Make = { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $runHome -Path (Get-RegistryPath)), [byte[]](0x7B, 0xFF, 0xFE, 0x7D)) } },
      @{ Name = 'a file that fails its schema (a record with no state)'; Error = $invalid; Make = { Write-RegistryText ('{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox"}]}' + "`n") } },
      @{ Name = 'a file that fails its schema (version 1)'; Error = $invalid; Make = { Write-RegistryText ('{"version":1,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}' + "`n") } },
      @{ Name = 'a file in which a member is named twice'; Error = $invalid; Make = { Write-RegistryText ('{"version":0,"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"}]}' + "`n") } },
      @{ Name = 'a file in which two records share a project'; Error = $invalid; Make = { Write-Registry @($good.Box, $good.P, $good.P) } },
      @{ Name = 'a file with no agentbox record'; Error = $invalid; Make = { Write-Registry @($good.P) } },
      @{ Name = 'a file with two agentbox records'; Error = $invalid; Make = { Write-Registry @($good.Box, [ordered]@{ project = 'other'; stack = 'agentbox'; state = 'ready' }) } },
      @{ Name = 'a file whose agentbox record''s project is not swimcoach'; Error = $invalid; Make = { Write-Registry @([ordered]@{ project = 'other'; stack = 'agentbox'; state = 'ready' }, $good.P) } },
      @{ Name = 'a file whose stack names another project'; Error = $invalid; Make = { Write-Registry @($good.Box, [ordered]@{ project = $pProject; stack = 'agentspace-host-swimcoach'; state = 'ready' }) } },
      @{ Name = 'a folder in its place'; Error = $null; Make = {
          Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $runHome -Path (Get-RegistryPath)) -Force -ErrorAction SilentlyContinue
          New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $runHome -Path (Get-RegistryPath)) | Out-Null } })
    foreach ($v in $variants) {
      & $v.Make
      $mark = (Read-LogLines).Count
      $code = Invoke-Keeper $now
      $log = Read-LogLines
      # The @() wraps the whole if, so that one new line is still an array and not a string.
      $new = @(if ($log.Count -gt $mark) { $log[$mark..($log.Count - 1)] })
      $line = if ($new.Count -eq 1) { $new[0] } else { '' }
      $reason = ''
      if ($line -cmatch ($logTime + 'failed lease=- alias=- pid=- reason=Host registry could not be read: (.*)\z')) { $reason = $Matches[1] }
      $label = $v.Name
      Add-HostCheck ($code -eq 1) "$label`: The pass exits 1" "The exit code was $code."
      Add-HostCheck ($new.Count -eq 1 -and $line -cmatch ($logTime + 'failed lease=- alias=- pid=- reason=Host registry could not be read: ')) "$label`: One failed line says the host registry could not be read, with no host field" "New lines: $($new -join ' | ')"
      if ($v.Error) { Add-HostCheck ($reason -ceq $v.Error) "$label`: The error is 'the host registry is not valid'" "The error was '$reason'." }
      else { Add-HostCheck ($reason -cmatch '\AWindows error \d+\.\z') "$label`: The error is 'Windows error <n>.', the number alone, and is not reported as not valid" "The error was '$reason'." }
      Add-HostCheck ((Test-TreeAlive $hb) -and (Test-TreeAlive $hp)) "$label`: Both holders are still running" 'A holder was stopped.'
      Add-HostCheck ([IO.File]::ReadAllText((Get-HostRecordPath $boxProject)) -ceq $textB -and [IO.File]::ReadAllText((Get-HostRecordPath $pProject)) -ceq $textP) "$label`: Both records are unchanged" 'A record was changed or removed.'
    }
  } }

  # H13. hosts.lock is never taken, opened or created by a pass.
  if (Test-Selected 'H13') { Invoke-HostCase 'H13' 'keeper.md#locks' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $boxProject 'b1' '2026-11-17T13:00:00Z'), (New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    $regPath = Get-RegistryPath
    $lockPath = Join-Path (Split-Path -Parent $regPath) 'hosts.lock'
    $stamp = { $i = Get-Item -LiteralPath $args[0] -Force; "$($i.Length)/$($i.LastWriteTimeUtc.Ticks)/$($i.CreationTimeUtc.Ticks)" }
    $bytesBefore = [IO.File]::ReadAllBytes($regPath)
    $stampBefore = & $stamp $regPath
    # Phase A: no hosts.lock. A pass reads the registry and never creates the lock.
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'With no hosts.lock: The pass exits 0' "The exit code was $code."
    Add-HostCheck (@(Select-ActionLines (Read-LogLines) 'started').Count -eq 2) 'With no hosts.lock: The pass starts both holders' "Log: $((Read-LogLines) -join ' | ')"
    Add-HostCheck (-not (Test-Path -LiteralPath $lockPath)) 'With no hosts.lock: A missing hosts.lock is never created' 'The pass created hosts.lock.'
    Add-HostCheck ((& $stamp $regPath) -ceq $stampBefore -and ([Convert]::ToBase64String([IO.File]::ReadAllBytes($regPath)) -ceq [Convert]::ToBase64String($bytesBefore))) 'With no hosts.lock: The registry''s bytes and time stamps are unchanged' 'The pass wrote the registry.'
    # Phase B: the case holds hosts.lock, with no sharing, for the whole pass.
    Reset-HostsState
    $held = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path $lockPath), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $lockStamp = & $stamp $lockPath
    $p = $null
    try {
      $p = Start-KeeperProcess @('-SshExe', $stub, '-Now', $now) 'keeper'
      $both = Wait-Until { @(Select-ActionLines (Read-LogLines) 'started').Count -ge 2 } 90 $p
      Add-HostCheck $both 'With hosts.lock held: The pass runs and starts both holders while the lock is held' "Started lines seen: $(@(Select-ActionLines (Read-LogLines) 'started').Count). The keeper $(if ($p.HasExited) { "ended with $($p.ExitCode)" } else { 'was still running' })."
      $ended = $p.WaitForExit(60000)
      Add-HostCheck ($ended -and $p.ExitCode -eq 0) 'With hosts.lock held: The pass exits 0' "$(if ($ended) { "The exit code was $($p.ExitCode)." } else { 'The pass had not ended 60 seconds after both starts.' })"
    } finally {
      $held.Dispose()
      if ($p -and -not $p.HasExited) { Stop-OwnTree $p.Id }
    }
    Add-HostCheck ((& $stamp $lockPath) -ceq $lockStamp) 'With hosts.lock held: hosts.lock is never written' 'The lock file''s length or time stamps changed.'
    Add-HostCheck ((& $stamp $regPath) -ceq $stampBefore -and ([Convert]::ToBase64String([IO.File]::ReadAllBytes($regPath)) -ceq [Convert]::ToBase64String($bytesBefore))) 'With hosts.lock held: The registry''s bytes and time stamps are unchanged' 'The pass wrote the registry.'
  } }

  # H14. Another host's holder log over 1 MiB is rotated, after the box's.
  if (Test-Selected 'H14') { Invoke-HostCase 'H14' 'keeper.md#the-holders-log' {
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @()
    $big = 1MB + 100
    $small = 1MB - 100
    foreach ($spec in @(@('board-holder-ssh.log', $big), @('board-holder-ssh.agentspace.log', $big), @('board-holder-ssh.swimcoach.log', $big), @('board-holder-ssh.zeta.log', $small))) {
      $fs = [IO.File]::Create((Assert-ConformancePath -ConformanceHome $runHome -Path (Join-Path $env:AGENTSPACE_STATE_DIR $spec[0])))
      try { $fs.SetLength($spec[1]) } finally { $fs.Dispose() }
    }
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck ($log.Count -eq 3 -and $log[0] -cmatch ($logTime + 'rotated lease=- alias=- pid=-\z') -and $log[1] -cmatch ($logTime + 'rotated lease=- alias=- pid=- host=agentspace\z') -and (Test-IdleLine $log[2])) 'The box''s rotated line (no host) comes first, then rotated host=agentspace, then the idle line' "Log: $($log -join ' | ')"
    $old = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.agentspace.log.1'))
    Add-HostCheck ($old.Exists -and $old.Length -eq $big -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.agentspace.log'))) 'agentspace''s log is renamed to board-holder-ssh.agentspace.log.1, whole' 'The log was not renamed.'
    $oldB = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.log.1'))
    Add-HostCheck ($oldB.Exists -and $oldB.Length -eq $big) 'The box''s log is renamed to board-holder-ssh.log.1, whole' 'The box''s log was not renamed.'
    $strayLog = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.swimcoach.log'))
    Add-HostCheck ($strayLog.Exists -and $strayLog.Length -eq $big -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.swimcoach.log.1'))) 'A board-holder-ssh.swimcoach.log is not another host''s log and is left alone' 'It was rotated.'
    $under = [IO.FileInfo]::new((Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.zeta.log'))
    Add-HostCheck ($under.Exists -and $under.Length -eq $small -and -not (Test-Path -LiteralPath (Join-Path $env:AGENTSPACE_STATE_DIR 'board-holder-ssh.zeta.log.1'))) 'A host''s log under 1 MiB is not rotated' 'It was rotated.'
  } }

  # H16. A record that cannot be removed ends its own host's turn and no other's.
  if (Test-Selected 'H16') { Invoke-HostCase 'H16' 'keeper.md#the-order-of-a-pass' {
    $r = $qProject
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject), (New-HostRecord $r))
    Write-Leases @((New-HostLease $r 'd1' '2026-11-17T13:00:00Z'))
    $hr = Start-HostHolder $r 'd1'
    $recordR = Get-HostRecordPath $r
    $beforeR = [IO.File]::ReadAllText($recordR)
    # agentspace's record names a process with the wrong start time, so it fails to be trusted, and the case holds it open so that it cannot be removed.
    # keeper.md H16: the record is held open sharing read and write but not delete, so the keeper reads it, it fails its trust check, and it
    # cannot be removed (architect, 2026-11-21, for v0.07-T04).
    $stub2 = Start-Stub (Get-HostAlias $pProject 'c1')
    Write-HostRecord $pProject $stub2.Pid '2001-01-01T00:00:00.000Z' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1')
    $held = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $runHome -Path (Get-HostRecordPath $pProject)), [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
    try { $code = Invoke-Keeper $now } finally { $held.Dispose() }
    Add-HostCheck ($code -eq 1) 'The one-pass run exits 1' "The exit code was $code."
    $log = Read-LogLines
    $failed = @($log | Where-Object { $_ -cmatch ($logTime + 'failed lease=- alias=- pid=- host=agentspace reason=The pass stopped on an error: The record could not be removed:') })
    Add-HostCheck ($failed.Count -eq 1) 'agentspace''s turn ends with its failed line: pass stopped on an error, the record could not be removed, with host=agentspace' "Log: $($log -join ' | ')"
    $iF = Get-FirstLineIndex $log ' failed '
    $iK = Get-FirstLineIndex $log ' kept '
    Add-HostCheck ($iK -ge 0 -and $iF -ge 0 -and $iK -gt $iF -and (Test-HostLine $log[$iK] 'kept' (Get-HostLeaseId 'd1') (Get-HostAlias $r 'd1') ([string]$hr.Pid) $r)) 'The later host''s turn still runs and keeps its holder, logging kept ... host=zeta after the failed line' "Log: $($log -join ' | ')"
    Add-HostCheck ((Test-TreeAlive $hr) -and (Test-Path -LiteralPath $recordR) -and [IO.File]::ReadAllText($recordR) -ceq $beforeR) 'The later host''s holder is running and its record is unchanged' 'It was stopped or changed.'
    Add-HostCheck ((Test-TreeAlive $stub2)) 'The process agentspace''s untrusted record names is left running' 'It was stopped.'
  } }

  # H18. AGENTSPACE_STATE_DIR does not move hosts.json.
  if (Test-Selected 'H18') { Invoke-HostCase 'H18' 'keeper.md#hosts' {
    $inState = Join-Path $env:AGENTSPACE_STATE_DIR 'hosts.json'
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $runHome -Path $inState), '{ this is not json')
    Write-Registry @((New-HostRecord $boxProject), (New-HostRecord $pProject))
    Write-Leases @((New-HostLease $pProject 'c1' '2026-11-17T13:00:00Z'))
    Add-HostCheck (-not [string]::Equals((Split-Path -Parent (Get-RegistryPath)), $env:AGENTSPACE_STATE_DIR, [StringComparison]::OrdinalIgnoreCase)) 'The state folder is not the folder that holds the registry' 'The fixture is wrong.'
    $code = Invoke-Keeper $now
    Add-HostCheck ($code -eq 0) 'The pass exits 0' "The exit code was $code."
    $log = Read-LogLines
    Add-HostCheck (@($log | Where-Object { $_ -match 'Host registry could not be read' }).Count -eq 0) 'No Host registry could not be read line is logged' "Log: $($log -join ' | ')"
    $recP = Read-HostRecord $pProject
    $st = @(Select-ActionLines $log 'started')
    Add-HostCheck ($log.Count -eq 2 -and $st.Count -eq 1 -and $null -ne $recP -and (Test-HostLine $st[0] 'started' (Get-HostLeaseId 'c1') (Get-HostAlias $pProject 'c1') ([string]$recP['pid']) $pProject)) 'The registry in USERPROFILE\.agentspace is read: a started line ends in host=agentspace' "Log: $($log -join ' | ')"
    Add-HostCheck ([IO.File]::ReadAllText($inState) -ceq '{ this is not json') 'The file in the state folder is never read as the registry or changed' 'It was changed.'
  } }
} finally {
  Restore-CaseEnvironment
  Stop-RunProcesses
  $leftBehind = @(Get-RunProcesses).Count
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath (Assert-ConformanceHomeRoot -Root $temp) -Recurse -Force -ErrorAction SilentlyContinue }
}

if ($leftBehind -eq 0) { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status PASS }
else { Write-ConformanceResult -Name 'keeper.md#checking-the-keeper: The run leaves no process behind' -Status FAIL -Reason "$leftBehind process(es) of this run were still running after the cleanup." }
Exit-ConformanceRun

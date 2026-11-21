#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which read no host registry and take no hosts.lock:
# every create answers `ready` at once there. A v0.07 build MUST pass every result line
# (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run (a program outside agentspace). See docs/conformance.md.
#
# Case of E0.07-T04-S01 (card #0155) that needs the host registry's lock held, named by its case in
# docs/driver.md, "Conformance cases for v0.07":
#   C22  With hosts.lock held by the check: a create for a listed project with no record, whose hold is let
#        go after under 30 seconds, succeeds as C5 says once it is let go; one whose hold lasts past 30
#        seconds fails with internal, records no lease, leaves the host registry as it was and starts
#        nothing.
# Sections enforced: driver.md "Where it places a lease" (the registry is read under its lock, waited for up
# to 30 seconds) and "Starting a provisioning run"; state.md "The host registry lock" and "The lease state
# lock" (the 30 seconds); lease-api.md "Concurrency and atomicity" and "Messages" (records busy).
#
# The lock is held by this check, a file opened with no sharing in the throwaway home, and let go either by
# a thread of this process after 8 seconds or only when the create has returned. The second case therefore
# takes the full 30 seconds of the wait, plus the 3-second settle that shows nothing was started.
#
# SAFETY. As the other files of this card: a throwaway home, a stand-in that makes no host, every write
# through Assert-ConformancePath at the call, and no failure reason that quotes a captured value.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-pending-create-lock.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 55 seconds.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$where = 'driver.md#where-it-places-a-lease'

Use-PvLab -Name "$where C22: With hosts.lock held and let go after 8 seconds, a create for a listed project with no record waits, then answers pending as C5 says and starts the stand-in once" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $stream = Open-PvLock -Lab $lab -Path (Get-PvHostsLockPath $lab)
  $job = Start-PvLockRelease -Stream $stream -AfterSeconds 8
  $before = Get-CcNowSecond
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  $after = Get-CcNowSecond
  [void](Wait-Job -Job $job -Timeout 30)
  Remove-Job -Job $job -Force
  $p = Get-PvPendingDocProblems -Run $run -Project 'agentspace'
  if ($run.Seconds -lt 6) { $p.Add("The create returned after $([int]$run.Seconds) seconds, before the lock was let go") }
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $lab)) { $p.Add($x) }
  $rec = Get-PvHostRecord $lab 'agentspace'
  if ($null -eq $rec) { $p.Add('hosts.json holds no record for the project') }
  else {
    $time = [string]$rec['last_attempt_at']
    if (-not (Test-PvTimeBetween -Time $time -Before $before -After $after)) { $p.Add('last_attempt_at is not within the command''s run') }
    $want = New-PvHostsText -Records @((New-PvHostRecord -Project 'agentspace' -State 'creating' -LastAttemptAt $time))
    foreach ($x in (Get-PvHostsTextProblems -Lab $lab -Expected $want)) { $p.Add($x) }
  }
  foreach ($x in (Get-PvStartedOnceProblems -StandIn $standIn -Project 'agentspace')) { $p.Add($x) }
  return , $p
}

Use-PvLab -Name "$where C22: With hosts.lock held past 30 seconds, a create for a listed project with no record waits 30 seconds, then fails with internal, records no lease, leaves the registry as it was and starts nothing" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  [void](Open-PvLock -Lab $lab -Path (Get-PvHostsLockPath $lab))
  $hash = Get-PvHostsHash $lab
  $run = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  $p = Get-PvInternalProblems -Run $run
  if ($run.Seconds -lt 27) { $p.Add("The create failed after $([int]$run.Seconds) seconds: it MUST keep trying for 30 seconds") }
  if ($run.Seconds -gt 40) { $p.Add("The create took $([int]$run.Seconds) seconds: it MUST NOT wait longer than 30") }
  foreach ($x in (Get-PvNothingRecordedProblems -Lab $lab)) { $p.Add($x) }
  if ((Get-PvHostsHash $lab) -ne $hash) { $p.Add('hosts.json was written') }
  foreach ($x in (Get-PvNoStartProblems -StandIn $standIn)) { $p.Add($x) }
  return , $p
}

Exit-ConformanceRun

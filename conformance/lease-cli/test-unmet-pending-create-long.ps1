#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the installed v0.06 programs, which start no run and answer `ready`. A v0.07
# build MUST pass every result line (docs/conformance.md, "The files that are expected to fail").
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what docs/lease-api.md, docs/driver.md and docs/state.md document, with a stand-in for the provisioning run (a program outside agentspace). See docs/conformance.md.
#
# Cases of E0.07-T04-S01 (card #0155) that need a stand-in which keeps running for a minute, each named by
# its case in docs/driver.md, "Conformance cases for v0.07":
#   C6   The create does not wait for the run: a stand-in that runs for a minute is still running after the
#        create has exited, and goes on to finish.
#   C29  The run inherits no lock: while that stand-in is still running, a second create in the same state
#        folder and a release of the first lease each succeed within their 30 seconds.
# Sections enforced: driver.md "Starting a provisioning run" (the detached start, "at most one run is
# started per start", "never waits for the run") and "Ordering and concurrency"; state.md "The lease state
# lock", "The host registry lock" and "The provisioning run's start"; lease-api.md "Concurrency and
# atomicity".
#
# The stand-in keeps running for 60 seconds after it recorded. Both cases share the one run, so the file
# takes a little over a minute. The stand-in is stopped, if it is still running when the checks end, by the
# PID it recorded, and only if the process still has the name and start time it recorded.
#
# SAFETY. As the other files of this card: a throwaway home, a stand-in that makes no host, every write
# through Assert-ConformancePath at the call, and no failure reason that quotes a captured value.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-pending-create-long.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.
# Time: about 70 seconds.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$start = 'driver.md#starting-a-provisioning-run'
$bound = 25   # Seconds. A command that waits for a lock does so up to 30 seconds and no longer.

Use-PvLab -Name "$start" -Body {
  param($lab)
  Set-PvProjects -Lab $lab -Names @('agentspace')
  $standIn = New-PvStandIn -Lab $lab -Name 'long' -SleepSeconds 60
  $c6 = [System.Collections.Generic.List[string]]::new()
  $c29 = [System.Collections.Generic.List[string]]::new()

  $first = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  foreach ($x in (Get-PvPendingDocProblems -Run $first -Project 'agentspace')) { $c6.Add($x) }
  if ($first.TimedOut) { $c6.Add('The create timed out') }
  if ($first.Seconds -ge $bound) { $c6.Add("The create took $([int]$first.Seconds) seconds: it MUST NOT wait for the run") }
  $n = Wait-PvStarted -StandIn $standIn -Count 1 -TimeoutSeconds 10
  if ($n -lt 1) { $c6.Add('The stand-in was not started within 10 seconds') }
  $calls = @(Wait-PvCalls -StandIn $standIn -Count 1 -TimeoutSeconds 40)
  if ($calls.Count -lt 1) { $c6.Add('The stand-in wrote no record within 40 seconds') }
  else {
    $c = $calls[0]
    if ($c['Ended']) { $c6.Add('The stand-in had already ended when the create returned') }
    if (-not (Test-PvRecordedAlive -ProcessId ([int]$c['pid']) -StartTicks ([long]$c['start_ticks']) -Names @('powershell'))) { $c6.Add('The stand-in is not running after the create has exited') }
  }

  # C29, while the stand-in is still running.
  $second = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', 'agentspace') -StandIn $standIn
  foreach ($x in (Get-PvPendingDocProblems -Run $second -Project 'agentspace')) { $c29.Add("second create: $x") }
  if ($second.Seconds -ge $bound) { $c29.Add("The second create took $([int]$second.Seconds) seconds: it waited for a lock the run holds") }
  if ($first.Json -is [hashtable] -and $first.Json['id'] -is [string]) {
    $release = Invoke-PvLease -Lab $lab -Arguments @('release', [string]$first.Json['id']) -StandIn $standIn
    foreach ($x in (Get-ConformanceSuccessProblems -Run $release)) { $c29.Add("release: $x") }
    if ($release.Json -is [hashtable] -and $release.Json['state'] -cne 'released') { $c29.Add("release: The lease's state is '$($release.Json['state'])', not released") }
    if ($release.Seconds -ge $bound) { $c29.Add("The release took $([int]$release.Seconds) seconds: it waited for a lock the run holds") }
  } else { $c29.Add('There is no first lease to release') }
  $n = @(Get-PvStartedLines $standIn).Count
  if ($n -ne 1) { $c29.Add("The stand-in was started $n times in all, not once") }

  # C6, the run goes on to finish: a bounded wait for the end of its minute.
  if ($calls.Count -ge 1) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
      $now = @(Get-PvCalls $standIn)
      $ended = ($now.Count -ge 1 -and $now[0]['Ended'])
      if (-not $ended) { Start-Sleep -Seconds 1 }
    } while (-not $ended -and $sw.Elapsed.TotalSeconds -lt 100)
    if (-not $ended) { $c6.Add('The stand-in did not go on to finish within 100 seconds') }
  }
  return [ordered]@{
    'C6: The create does not wait for the run: a stand-in that runs for a minute is still running after the create has exited, and goes on to finish' = $c6
    'C29: While the run is still running, a second create in the same state folder and a release of the first lease each succeed within 30 seconds, so the run inherits no lock' = $c29
  }
}

Exit-ConformanceRun

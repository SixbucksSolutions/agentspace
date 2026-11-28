# The drafter's clean dry run of the v0.12-T05 linux/amd64 check: every step script, in order, in one conformance-harness home,
# against the stand-ins' clean answers, each through scripts\live\wrap.ps1 into target\runs\drafter. Drafter-owned.
# RFC 2119 keywords; the capitals are load-bearing. Run it through scripts\live\wrap.ps1 too. -Base is the number of the first
# inner wrapper. Exit 0 when every step exits 0 as designed.
param([int]$Base = 11, [string]$Tag = 'clean')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-drafter-lib.ps1')
$lab = New-S01Lab
$bad = 0
$n = $Base
try {
  Enter-S01Home -Lab $lab
  Write-Host "Lab ready. CaReady: $($lab.CaReady) (no CA is wanted). TEMP is under target: $($env:TEMP.StartsWith((Join-Path $script:DRepo 'target')))."
  $case = New-S01CaseDir $Tag
  Write-S01HomeLogs -Lab $lab -Provision $script:DProvisionOk -Guard @('2026-11-27T20:00:01Z host=agentspace unexplained since=2026-11-27T19:00:00Z stops=2026-11-27T19:15:00Z', '2026-11-27T20:05:01Z host=agentspace stopped since=2026-11-27T19:00:00Z', '2026-11-27T20:10:01Z failed step=describe reason=sign-in has expired')
  Write-S01Observe -Case $case -OpenMinutes 20
  Initialize-S01Flow -Case $case -Leases none -Phase before
  $run = {
    param([string]$Step, [string]$What, [string[]]$Must, [string[]]$MustNot = @(), [string]$Phase = '')
    if ($Phase) { Write-S01File (Join-Path $case 'standin\phase.txt') $Phase }
    $name = ('{0:D3}-clean-{1}-{2}' -f $script:n, $Step, $What); $script:n++
    $r = Invoke-S01Case -Lab $lab -Case $case -Name $name -Step $Step -ExpectExit 0 -Must (@('PASS: every check of this step held.') + $Must) -MustNot (@('FINDING:') + $MustNot)
    if (-not $r.Ok) { $script:bad++ }
  }
  & $run '200' 'maskproof' @('Mask proof: 291 passed, 0 failed.', 'Environment lines reading True: 4')
  & $run '201' 'aws' @('AWS sign-in: good (exit 0).') @('AIDA', 'PLANTED', '<account-id>', '<arn>')
  & $run '202' 'status-before' @('Hosts read: 2; State lines: 2; Leases lines: 2; Guard lines: 0', "linux/arm64 host 'swimcoach' (agentbox) listed: True", "linux/arm64 host 'agentspace' (agentspace-host-agentspace) listed: True", 'Total leases: 0', 'guard.log lines now: 3') @() 'before'
  & $run '210' 'create-a' @("State: 'pending'", "size: 'm'", "ttl_seconds: '7200'", 'ssh: absent', 'Hours of linux/amd64 by the observer: 0.') $script:DNoLease
  & $run '211' 'wait' @('Pass 1 at', 'Pass 3 at', "state 'ready'", 'Lease A is ready after 3 read(s)', 'provision.log: 5 lines', 'run done') $script:DNoLease
  & $run '212' 'get-a' @("State: 'ready'", "ssh.user: 'ubuntu'", 'ssh.host_alias present: True', 'ssh.host_alias is agentspace-<project>-<first 8 hex digits of id>: True', 'Schema lease.output.schema.json: True') $script:DNoLease
  & $run '213' 'ssh' @("Alias read from the state file: True (never printed)", 'a proxycommand line exists: True', 'ssh <alias> true: exit 0', "prints 'x86_64'", 'glibc 2.43 is 2.43 or newer, compared as numbers: True', 'Tools answering: 6 of 6', 'tool node: answered (direct form, exit 0): v26.13.1', 'tool gh: answered (direct form, exit 0): gh version 2.105.0') @('user ubuntu', 'proxycommand C:', 'hostname', 'https://github.com') 'running'
  & $run '214' 'status-host' @("Platform 'linux/amd64'; State 'running'; Type 'm8a.xlarge'", 'Instance line: True; Disk line: True', "SSM 'online'") @('NOTE:')
  & $run '220' 'create-b' @("size: 's'", "State: 'ready'", "Lease B's alias differs from lease A's: True", "Type 'm8a.xlarge'", 'ready: 2 (expected 2: A and B)') $script:DNoLease
  & $run '230' 'release' @('Release of lease B finished', 'Release of lease A finished', 'as expected', 'Leases A and B listed as released: 2 of 2', 'guard.log lines before the releases: 3', "the guard stops the host by") $script:DNoLease
  # What the guard and the observer leave after 230: new guard.log lines, and the stopped edge 9 minutes after the end.
  $end = [DateTimeOffset]::UtcNow
  Write-S01HomeLogs -Lab $lab -Provision $script:DProvisionOk -Guard @(
    '2026-11-27T20:00:01Z host=agentspace unexplained since=2026-11-27T19:00:00Z stops=2026-11-27T19:15:00Z', '2026-11-27T20:05:01Z host=agentspace stopped since=2026-11-27T19:00:00Z', '2026-11-27T20:10:01Z failed step=describe reason=sign-in has expired',
    ((Format-DTime $end.AddMinutes(8)) + ' host=agentspace.linux-amd64 unexplained since=' + (Format-DTime $end) + ' stops=' + (Format-DTime $end.AddMinutes(15))),
    ((Format-DTime $end.AddMinutes(9)) + ' host=agentspace.linux-amd64 stopped since=' + (Format-DTime $end)))
  Write-S01Observe -Case $case -OpenMinutes 25 -Edges @((New-S01EdgeLine $end.AddMinutes(9)))
  & $run '240' 'guard-stop' @('a stopped edge of agentspace.linux-amd64 after step 230''s end: True', 'Within D56''s bound of 25 minutes: True', 'guard.log lines now: 5 (baseline 3, new 2)', 'New guard.log lines that say agentspace.linux-amd64 stopped: 1', 'Attribution: an API stop', "State.Name 'stopped', StateReason.Code 'Client.UserInitiatedShutdown'") $script:DNoLease 'after'
  & $run '250' 'status-after' @('Hosts read: 3; State lines: 3; Leases lines: 3; Guard lines: 0', 'agentspace.linux-amd64 listed: 1', 'Platform  linux/amd64') @() 'after'
} finally {
  Restore-S01Env
  Remove-ConformanceHome -ConformanceHome $lab
  Write-Host "Home removed: $(-not (Test-Path -LiteralPath $lab.Root)); tripwire starts: $(Get-ConformanceTripwireStarts -ConformanceHome $lab)"
}
$ok = ($bad -eq 0)
Write-Host "Clean dry run: $($n - $Base) steps run; steps not as designed: $bad"
if ($ok) { exit 0 }
exit 1

# The drafter's planted dry runs of the v0.12-T05 linux/amd64 check: each checker of each step shown to fail (or stop) on one
# planted stand-in answer, through scripts\live\wrap.ps1 into target\runs\drafter, in one conformance-harness home.
# Drafter-owned. RFC 2119 keywords; the capitals are load-bearing. Run it through scripts\live\wrap.ps1 too.
# A planted case is named planted-<step>-<what>; a control (a variation that MUST still pass) control-<step>-<what>.
# Exit 0 when every case went as designed.
param([int]$Base = 101)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-drafter-lib.ps1')
$lab = New-S01Lab
$script:n = $Base
$script:bad = 0
$script:cases = 0
$script:planted = 0
$script:controls = 0
$noLease = $script:DNoLease

$baseGuard = @('2026-11-27T20:00:01Z host=agentspace unexplained since=2026-11-27T19:00:00Z stops=2026-11-27T19:15:00Z', '2026-11-27T20:05:01Z host=agentspace stopped since=2026-11-27T19:00:00Z', '2026-11-27T20:10:01Z failed step=describe reason=sign-in has expired')

# State and logs of step 240's entry: step 230 ended $EndAgo minutes ago, with guard.log at 3 lines; $Guard are the new
# lines (as '<minutes after the end> <text>'), $Edges the observer's edges (as '<minutes after the end>|<host>|<from>').
function Set-S01Entry240 {
  param([string]$Case, $Lab, [double]$EndAgo = 10, [string[]]$Guard = @(), [string[]]$Edges = @(), [string]$Hours = 'ok', [double]$Open = 25)
  $end = [DateTimeOffset]::UtcNow.AddMinutes(-$EndAgo)
  Write-S01File (Join-Path $Case 'state\end-230.utc') (Format-DTime $end)
  Write-S01File (Join-Path $Case 'state\guard-230.count') '3'
  $g = @($baseGuard)
  foreach ($x in $Guard) { $m, $t = $x -split ' ', 2; $g += ((Format-DTime $end.AddMinutes([double]$m)) + ' ' + $t.Replace('{since}', (Format-DTime $end))) }
  Write-S01HomeLogs -Lab $Lab -Provision $script:DProvisionOk -Guard $g
  $e = @()
  foreach ($x in $Edges) { $m, $h, $f = $x -split '\|'; $e += ('{0} 9:50pm {1} {2} -> stopped type=m8a.xlarge' -f (Format-DTime $end.AddMinutes([double]$m)), $h, $f) }
  Write-S01Observe -Case $Case -OpenMinutes $Open -Hours $Hours -Edges $e
}
$okGuard = @('9 host=agentspace.linux-amd64 stopped since={since}')
$okEdge = @('9|agentspace.linux-amd64|stopping')

function Add-S01Case {
  param(
    [string]$Step, [string]$What, [string]$Plant = '', [string]$Leases = 'none', [string]$Phase = 'before', [int]$Exit = 1,
    [string[]]$Must = @(), [string[]]$MustNot = @(), [scriptblock]$Prep, [hashtable]$EnvVars = @{}, [switch]$Live,
    [double]$Open = 20, [string]$Hours = 'ok', [switch]$Control
  )
  $kind = if ($Control) { 'control' } else { 'planted' }
  $name = ('{0:D3}-{1}-{2}-{3}' -f $script:n, $kind, $Step, $What); $script:n++
  $case = New-S01CaseDir ("$Step-$What")
  Write-S01Observe -Case $case -OpenMinutes $Open -Hours $Hours
  Initialize-S01Flow -Case $case -Leases $Leases -Phase $Phase
  Write-S01HomeLogs -Lab $lab -Provision $script:DProvisionOk -Guard $baseGuard
  if ($Prep) { & $Prep $case }
  $r = Invoke-S01Case -Lab $lab -Case $case -Name $name -Step $Step -Plant $Plant -Live:$Live -EnvVars $EnvVars -ExpectExit $Exit -Must $Must -MustNot $MustNot
  $script:cases++
  if ($Control) { $script:controls++ } else { $script:planted++ }
  if (-not $r.Ok) { $script:bad++ }
}

try {
  Enter-S01Home -Lab $lab
  Write-Host "Lab ready. CaReady: $($lab.CaReady) (no CA is wanted)."
  # --- 200
  Add-S01Case 200 'maskproof-fail' 'maskproof-fail' -Must @('mask-proof.ps1 exited 1', '1 FAILED: line(s)')
  Add-S01Case 200 'maskproof-false' 'maskproof-false' -Must @('the four environment lines')
  Add-S01Case 200 'maskproof-nosummary' 'maskproof-nosummary' -Must @('is not there exactly once')
  # --- 201
  Add-S01Case 201 'sts-expired' 'sts-expired' -Must @('The AWS sign-in is not good (exit 255)', 'aws login --remote', 'Incognito') -MustNot @('Token has expired', 'refresh failed')
  # --- 202
  Add-S01Case 202 'arm64-running' 'all-running-arm64' -Must @("host agentspace is 'running', not stopped", 'has a Guard line')
  Add-S01Case 202 'box-missing' 'all-missing-box' -Must @("the linux/arm64 host 'swimcoach' is not listed once")
  Add-S01Case 202 'box-pending' 'all-box-pending' -Must @("host swimcoach is 'pending', not stopped")
  Add-S01Case 202 'leases-live' 'all-leases-live' -Must @('has 1 live lease(s), expected 0')
  Add-S01Case 202 'no-leases-line' 'all-no-leases-line' -Must @('has no Leases line')
  Add-S01Case 202 'status-exit1' 'all-exit1' -Must @('status -All exited 1')
  Add-S01Case 202 'status-empty' 'all-empty' -Must @('no host was read')
  Add-S01Case 202 'amd-running' 'all-amd-running' -Phase running -Must @("host agentspace.linux-amd64 is 'running', not stopped")
  Add-S01Case 202 'lease-file-note' 'all-lease-file-note' -Must @('status says the lease state file cannot be read')
  Add-S01Case 202 'list-other-ready' 'list-other-ready' -Must @('1 lease(s) are ready or pending anywhere in the list')
  Add-S01Case 202 'list-invalid' 'list-invalid' -Must @('the list does not validate against lease-list.output.schema.json')
  Add-S01Case 202 'amd-before-stopped' 'all-amd-before' -Exit 0 -Control -Must @('PASS: every check of this step held.', 'agentspace.linux-amd64 listed: 1')
  # --- 210
  Add-S01Case 210 'create-ready' 'create-ready' -Must @("state is 'ready', expected 'pending'", 'ready at once') -MustNot $noLease
  Add-S01Case 210 'create-platform' 'create-platform' -Must @("platform is 'linux/arm64', expected 'linux/amd64'") -MustNot $noLease
  Add-S01Case 210 'create-size' 'create-size' -Must @("size is 'l', expected 'm'") -MustNot $noLease
  Add-S01Case 210 'create-ttl' 'create-ttl' -Must @("ttl_seconds is '28800', expected 7200") -MustNot $noLease
  Add-S01Case 210 'create-profile' 'create-profile' -Must @("profile is 'agent-windows', expected 'agent-linux'") -MustNot $noLease
  Add-S01Case 210 'create-member' 'create-member' -Must @('members are') -MustNot $noLease
  Add-S01Case 210 'create-project' 'create-project' -Must @("project is 'swimcoach', expected 'agentspace'") -MustNot $noLease
  Add-S01Case 210 'create-bad-id' 'create-bad-id' -Must @('id is not a lowercase UUIDv4') -MustNot $noLease
  Add-S01Case 210 'create-unsat' 'create-unsat' -Must @("state is 'unsatisfiable', expected 'pending'", 'Reason 1: code=no_capacity') -MustNot $noLease
  Add-S01Case 210 'create-exit' 'create-exit' -Must @('exit 1, expected 0', 'Error document (masked):')
  Add-S01Case 210 'create-stderr' 'create-stderr' -Must @('stderr holds') -MustNot $noLease
  Add-S01Case 210 'create-pretty' 'create-pretty' -Must @('stdout is not one line ending in one line feed') -MustNot $noLease
  Add-S01Case 210 'hours-high' '' -Open 130 -Exit 6 -Must @('at or past 1.75') -MustNot @('Command:')
  Add-S01Case 210 'hours-missing' '' -Hours missing -Exit 6 -Must @('cannot be read', 'does not exist') -MustNot @('Command:')
  Add-S01Case 210 'hours-stale' '' -Hours stale -Exit 6 -Must @('has not been written for') -MustNot @('Command:')
  Add-S01Case 210 'already-recorded' '' -Leases pending -Must @('already recorded') -MustNot @('Command:')
  Add-S01Case 210 'hours-arm64-not-counted' '' -Exit 0 -Control -Open 20 -Must @('PASS: every check of this step held.', 'Hours of linux/amd64 by the observer: 0.3')
  # --- 211 (lease A pending in the store)
  Add-S01Case 211 'get-unsat' 'get-unsat' -Leases pending -Exit 2 -Must @("is 'unsatisfiable', not pending or ready")
  Add-S01Case 211 'get-expired' 'get-expired' -Leases pending -Exit 2 -Must @("is 'expired', not pending or ready")
  Add-S01Case 211 'bound' 'get-pending' -Leases pending -Exit 3 -EnvVars @{ S01_BOUND_SECONDS = '3' } -Must @('Bound: 3 seconds', 'still pending after', 'at the bound of 3 seconds')
  Add-S01Case 211 'get-exit' 'get-exit' -Leases pending -Must @('lease get did not print a lease')
  Add-S01Case 211 'hours-high' '' -Leases pending -Open 130 -Exit 6 -Must @('at or past 1.75') -MustNot @('Pass 1 at')
  Add-S01Case 211 'hours-missing' '' -Leases pending -Hours missing -Exit 6 -Must @('cannot be read')
  Add-S01Case 211 'no-lease' '' -Must @('holds no lease A')
  Add-S01Case 211 'sync-failed' '' -Leases pending -Exit 1 -Must @("1 of them 'sync failed'", 'Lease A is ready after 3 read(s)', 'MUST NOT fail at sync') -Prep {
    param($c) Write-S01HomeLogs -Lab $lab -Provision @('2026-11-27T21:40:01Z run started project=agentspace platform=linux/amd64', '2026-11-27T21:55:00Z sync failed reason=a made-up reason', '2026-11-27T21:56:00Z run failed') -Guard $baseGuard
  }
  Add-S01Case 211 'patch-failed-note' '' -Leases pending -Exit 0 -Control -Must @('NOTE: provision.log says the patch run failed', 'Lease A is ready after 3 read(s)') -Prep {
    param($c) Write-S01HomeLogs -Lab $lab -Provision @('2026-11-27T21:46:30Z sync done; the patch run failed, see the host''s journal', '2026-11-27T21:47:01Z run done') -Guard $baseGuard
  }
  Add-S01Case 211 'log-missing' '' -Leases pending -Exit 0 -Control -Must @('provision.log: not there yet', 'Lease A is ready after 3 read(s)') -Prep {
    param($c) Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ('.agentspace\hosts\' + $script:DAmd + '\provision.log'))) -Force
  }
  # --- 212 (lease A ready)
  Add-S01Case 212 'get-pending' 'get-state-pending' -Leases ready -Must @("state is 'pending', expected 'ready'") -MustNot $noLease
  Add-S01Case 212 'get-unsat' 'get-unsat' -Leases ready -Must @("state is 'unsatisfiable', expected 'ready'") -MustNot $noLease
  Add-S01Case 212 'get-expired' 'get-expired' -Leases ready -Must @("state is 'expired', expected 'ready'") -MustNot $noLease
  Add-S01Case 212 'get-user' 'get-user' -Leases ready -Must @("ssh.user is 'root', expected 'ubuntu'") -MustNot $noLease
  Add-S01Case 212 'get-noalias' 'get-noalias' -Leases ready -Must @('ssh.host_alias is absent or empty') -MustNot $noLease
  Add-S01Case 212 'get-alias-form' 'get-alias-form' -Leases ready -Must @('ssh.host_alias is not of the form') -MustNot $noLease
  Add-S01Case 212 'get-size' 'get-size' -Leases ready -Must @("size is 'l', expected 'm'") -MustNot $noLease
  Add-S01Case 212 'get-profile' 'get-profile' -Leases ready -Must @("profile is 'agent-windows'") -MustNot $noLease
  Add-S01Case 212 'get-platform' 'get-platform' -Leases ready -Must @("platform is 'linux/arm64'") -MustNot $noLease
  Add-S01Case 212 'get-ttl' 'get-ttl' -Leases ready -Must @("ttl_seconds is '3600', expected 7200") -MustNot $noLease
  Add-S01Case 212 'get-expires-past' 'get-expires-past' -Leases ready -Must @('expires_at is not in the future') -MustNot $noLease
  Add-S01Case 212 'get-expires-far' 'get-expires-far' -Leases ready -Must @('more than the TTL ahead') -MustNot $noLease
  Add-S01Case 212 'get-no-ssh' 'get-no-ssh' -Leases ready -Must @('the lease has no ssh object') -MustNot $noLease
  Add-S01Case 212 'get-exit' 'get-exit' -Leases ready -Must @('exit 1, expected 0')
  Add-S01Case 212 'unknown-lease' '' -Leases ready -Must @('exit 3, expected 0') -MustNot $noLease -Prep {
    param($c) Write-S01File (Join-Path $c 'state\lease-A.id') ('11111111' + '-1111-4111-8111-' + '1111' + '11111111')
  }
  # --- 213 (lease A ready, the host running)
  Add-S01Case 213 'g-no-proxy' 'g-no-proxy' -Leases ready -Phase running -Must @('ssh -G shows no proxycommand line') -MustNot @('user ubuntu', 'ssh <alias> true')
  Add-S01Case 213 'g-exit1' 'g-exit1' -Leases ready -Phase running -Must @('ssh -G exited 1')
  Add-S01Case 213 'true-fails' 'true-fails' -Leases ready -Phase running -Must @('the real login (ssh <alias> true) exited 255', 'Permission denied') -MustNot @('uname -m:')
  Add-S01Case 213 'uname-arm' 'uname-arm' -Leases ready -Phase running -Must @("uname -m prints 'aarch64', expected 'x86_64'")
  Add-S01Case 213 'ldd-242' 'ldd-242' -Leases ready -Phase running -Must @('glibc is 2.42, expected 2.43 or newer')
  Add-S01Case 213 'ldd-25' 'ldd-25' -Leases ready -Phase running -Must @('glibc is 2.5, expected 2.43 or newer')
  Add-S01Case 213 'ldd-noversion' 'ldd-noversion' -Leases ready -Phase running -Must @('ends in no version number')
  Add-S01Case 213 'node-missing' 'missing-node' -Leases ready -Phase running -Must @('tool node: NOT answered', 'the tool node does not answer')
  Add-S01Case 213 'fnm-missing' 'missing-fnm' -Leases ready -Phase running -Must @('tool fnm: NOT answered')
  Add-S01Case 213 'uv-noversion' 'noversion-uv' -Leases ready -Phase running -Must @('tool uv: NOT answered')
  Add-S01Case 213 'gh-empty' 'empty-gh' -Leases ready -Phase running -Must @('tool gh: NOT answered')
  Add-S01Case 213 'claude-exit1' 'exit1-claude' -Leases ready -Phase running -Must @('tool claude: NOT answered')
  Add-S01Case 213 'codex-missing' 'missing-codex' -Leases ready -Phase running -Must @('tool codex: NOT answered')
  Add-S01Case 213 'hours-high' '' -Leases ready -Phase running -Open 130 -Exit 6 -Must @('at or past 1.75') -MustNot @('ssh -G')
  Add-S01Case 213 'no-doc' '' -Leases ready -Phase running -Must @('doc-212.json is not in the state folder') -Prep { param($c) Remove-Item -LiteralPath (Join-Path $c 'state\doc-212.json') -Force }
  Add-S01Case 213 'bad-alias' '' -Leases ready -Phase running -Must @('holds no usable host alias') -Prep { param($c) Write-S01File (Join-Path $c 'state\doc-212.json') ('{"ssh":{"host_alias":"-oProxyCommand=x","user":"ubuntu"}}' + "`n") }
  Add-S01Case 213 'ldd-3-passes' 'ldd-3' -Leases ready -Phase running -Exit 0 -Control -Must @('glibc 3.0 is 2.43 or newer, compared as numbers: True', 'PASS: every check of this step held.')
  Add-S01Case 213 'node-login-only' 'login-only-node' -Leases ready -Phase running -Exit 0 -Control -Must @('tool node: answered (login shell form, exit 0)', 'NOTE: node is found by an interactive login shell only', 'Tools answering: 6 of 6')
  # --- 214
  Add-S01Case 214 'host-stopped' 'host-stopped' -Phase running -Must @("State is 'stopped', expected 'running'")
  Add-S01Case 214 'host-type' 'host-type' -Phase running -Must @("Type is 'm8a.large', expected 'm8a.xlarge'")
  Add-S01Case 214 'host-no-platform' 'host-no-platform' -Phase running -Must @("Platform is '', expected 'linux/amd64'")
  Add-S01Case 214 'host-exit1' 'host-exit1' -Phase running -Must @('status exited 1, expected 0')
  Add-S01Case 214 'host-no-instance' 'host-no-instance' -Phase running -Must @('no Instance line')
  Add-S01Case 214 'ssm-note' 'host-ssm' -Phase running -Exit 0 -Control -Must @('NOTE: SSM reads ''not yet''', 'PASS: every check of this step held.')
  # --- 220 (lease A ready, the host running)
  Add-S01Case 220 'b-pending' 'b-pending' -Leases ready -Phase running -Must @("state is 'pending', expected 'ready'", 'D66''s second row', 'MAY be under way') -MustNot $noLease
  Add-S01Case 220 'b-size' 'b-size' -Leases ready -Phase running -Must @("size is 'm', expected 's'") -MustNot $noLease
  Add-S01Case 220 'b-user' 'b-user' -Leases ready -Phase running -Must @("ssh.user is 'root', expected 'ubuntu'") -MustNot $noLease
  Add-S01Case 220 'b-same-alias' 'b-same-alias' -Leases ready -Phase running -Must @('lease B has the same alias as lease A') -MustNot $noLease
  Add-S01Case 220 'host-resized' 'host-resized' -Leases ready -Phase running -Must @("Type is 'm8a.2xlarge', expected 'm8a.xlarge'") -MustNot $noLease
  Add-S01Case 220 'host-stopped' 'host-stopped' -Leases ready -Phase running -Must @("State is 'stopped', expected 'running'") -MustNot $noLease
  Add-S01Case 220 'list-extra-ready' 'list-ours-ready' -Leases ready -Phase running -Must @('3 lease(s) of the project on linux/amd64 are ready, expected 2') -MustNot $noLease
  Add-S01Case 220 'create-exit' 'create-exit' -Leases ready -Phase running -Must @('exit 1, expected 0')
  Add-S01Case 220 'hours-high' '' -Leases ready -Phase running -Open 130 -Exit 6 -Must @('at or past 1.75') -MustNot @('Command:')
  Add-S01Case 220 'no-a' '' -Phase running -Must @('holds no lease A')
  Add-S01Case 220 'b-recorded' '' -Leases two -Phase running -Must @('A lease B is already recorded') -MustNot @('Command:')
  # --- 230 (leases A and B ready)
  Add-S01Case 230 'release-ready' 'release-ready' -Leases two -Phase running -Must @("lease B: state is 'ready', expected 'released'", "lease A: state is 'ready', expected 'released'", 'Release of lease A finished') -MustNot $noLease
  Add-S01Case 230 'release-ssh' 'release-ssh' -Leases two -Phase running -Must @('lease B: ', 'ssh member') -MustNot $noLease
  Add-S01Case 230 'release-no-time' 'release-no-time' -Leases two -Phase running -Must @('released_at is absent or not a time') -MustNot $noLease
  Add-S01Case 230 'release-exit-b' 'release-exit-b' -Leases two -Phase running -Must @('lease B: exit 1, expected 0', 'Release of lease A finished', 'Release of lease B finished') -MustNot $noLease
  Add-S01Case 230 'list-ours-ready' 'list-ours-ready' -Leases two -Phase running -Must @('still ready or pending') -MustNot $noLease
  Add-S01Case 230 'no-lease' '' -Must @('there is nothing to release')
  Add-S01Case 230 'no-b' '' -Leases ready -Phase running -Must @('the state file holds no lease B') -MustNot $noLease
  # --- 240
  Add-S01Case 240 'no-edge' '' -Exit 3 -Phase after -Must @('no stopped edge of agentspace.linux-amd64 was seen by the bound') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard } -EnvVars @{ S01_BOUND_SECONDS = '2' }
  Add-S01Case 240 'edge-before-end' '' -Exit 3 -Phase after -Must @('stopped edge of agentspace.linux-amd64 after step 230''s end: False') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges @('-5|agentspace.linux-amd64|stopping') } -EnvVars @{ S01_BOUND_SECONDS = '2' }
  Add-S01Case 240 'edge-other-host' '' -Exit 3 -Phase after -Must @('after step 230''s end: False') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges @('9|agentspace|stopping') } -EnvVars @{ S01_BOUND_SECONDS = '2' }
  Add-S01Case 240 'edge-late' '' -Exit 1 -Phase after -Must @('past D56''s 25 minutes') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -EndAgo 33 -Guard $okGuard -Edges @('30|agentspace.linux-amd64|stopping') }
  Add-S01Case 240 'no-guard-line' '' -Exit 1 -Phase after -Must @('holds no new line for host=agentspace.linux-amd64 stopped') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Edges $okEdge }
  Add-S01Case 240 'guard-line-failed' '' -Exit 1 -Phase after -Must @('holds no new line for host=agentspace.linux-amd64 stopped', 'a new guard.log line is a failure') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard @('9 host=agentspace.linux-amd64 failed step=stop reason=sign-in has expired') -Edges $okEdge }
  Add-S01Case 240 'guard-line-other-host' '' -Exit 1 -Phase after -Must @('holds no new line for host=agentspace.linux-amd64 stopped') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard @('9 host=agentspace stopped since={since}') -Edges $okEdge }
  Add-S01Case 240 'host-own-power-off' 'ec2-host-own' -Exit 1 -Phase after -Must @('the host''s own power-off') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'ec2-running' 'ec2-running' -Exit 1 -Phase after -Must @("the instance is 'running', expected 'stopped'") -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'ec2-two' 'ec2-two' -Exit 1 -Phase after -Must @('2 instance(s) of the stack are not terminated, expected 1') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'ec2-empty' 'ec2-empty' -Exit 1 -Phase after -Must @('0 instance(s) of the stack are not terminated, expected 1', 'Instances read: 0') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'ec2-fail' 'ec2-fail' -Exit 1 -Phase after -Must @('aws ec2 describe-instances exited 255') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'ec2-other-code' 'ec2-other-code' -Exit 1 -Phase after -Must @("StateReason.Code is 'Server.SpotInstanceTermination'") -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'hours-high' '' -Exit 6 -Phase after -Must @('at or past 1.75') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge -Open 130 }
  Add-S01Case 240 'hours-stale' '' -Exit 6 -Phase after -Must @('has not been written for') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge -Hours stale }
  Add-S01Case 240 'no-end' '' -Exit 1 -Phase after -Must @('holds no end time')
  Add-S01Case 240 'edge-25-5' '' -Exit 0 -Control -Phase after -Must @('NOTE: seen 25 to 26 minutes after the end', 'PASS: every check of this step held.') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -EndAgo 30 -Guard @('25.5 host=agentspace.linux-amd64 stopped since={since}') -Edges @('25.5|agentspace.linux-amd64|stopping') }
  Add-S01Case 240 'second-filter' 'ec2-second-form' -Exit 0 -Control -Phase after -Must @('Instances read: 1; filter used: Name=tag:project', 'PASS: every check of this step held.') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  Add-S01Case 240 'terminated-old' 'ec2-terminated-old' -Exit 0 -Control -Phase after -Must @('PASS: every check of this step held.') -Prep { param($c) Set-S01Entry240 -Case $c -Lab $lab -Guard $okGuard -Edges $okEdge }
  # --- 250 (all stopped, the amd64 host made)
  Add-S01Case 250 'arm64-running' 'all-running-arm64' -Phase after -Must @("host agentspace is 'running', not stopped")
  Add-S01Case 250 'amd-absent' 'all-amd-absent' -Phase after -Must @('agentspace.linux-amd64 is not listed once, so the check did not make it')
  Add-S01Case 250 'wrong-stack' 'all-wrong-stack' -Phase after -Must @('is not stack agentspace-linux-amd64-host-agentspace')
  Add-S01Case 250 'leases-live' 'all-leases-live' -Phase after -Must @('has 1 live lease(s), expected 0')
  Add-S01Case 250 'amd-running' 'all-amd-running' -Phase after -Must @("host agentspace.linux-amd64 is 'running', not stopped")
  Add-S01Case 250 'list-other-ready' 'list-other-ready' -Phase after -Must @('1 lease(s) are ready or pending anywhere in the list')
  Add-S01Case 250 'status-exit1' 'all-exit1' -Phase after -Must @('status -All exited 1')
  Add-S01Case 250 'hours-high' '' -Phase after -Open 130 -Exit 6 -Must @('at or past 1.75')
  # --- the live code path: no S01_DRY, no stand-ins, the home's own installed-program paths
  Add-S01Case 202 'live-env-stop' '' -Live -Exit 4 -Phase after -EnvVars @{ S01_AGENTSPACE = (Join-Path $script:DDir 's01-standin-agentspace.ps1') } -Must @('holds 1 dry-run or AGENTSPACE_* variable(s): S01_AGENTSPACE') -MustNot @('Command:', 'Hosts read')
  Add-S01Case 210 'live-agentspace-var-stop' '' -Live -Exit 4 -EnvVars @{ AGENTSPACE_PLANT = '1' } -Must @('AGENTSPACE_PLANT') -MustNot @('Command:')
  Add-S01Case 202 'live-path-in-home' '' -Live -Exit 1 -Phase after -Must @('the operator could not be started', 'the program could not be started') -MustNot @('Hosts read: 2', 'DRY RUN')
} finally {
  Restore-S01Env
  $starts = Get-ConformanceTripwireStarts -ConformanceHome $lab
  Remove-ConformanceHome -ConformanceHome $lab
  Write-Host "Home removed: $(-not (Test-Path -LiteralPath $lab.Root)); tripwire starts: $starts"
}
Write-Host "Planted dry runs: $($script:cases) cases ($($script:planted) planted, $($script:controls) controls); not as designed: $($script:bad)"
if ($script:bad -eq 0) { exit 0 }
exit 1

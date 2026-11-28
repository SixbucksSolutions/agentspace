# The drafter's dry run of the v0.12-T05 lease steps against this worktree's own build (target\dist\agentspace.exe, built at
# f1dd444, NOT rebuilt), in two conformance-harness homes. Drafter-owned. RFC 2119 keywords; the capitals are load-bearing.
# Run it through scripts\live\wrap.ps1 too. -Base is the number of the first inner wrapper.
#
# - Home 1 holds a project list that approves agentspace on linux/amd64 and no host record: step 210's `create` MUST answer
#   pending from the real build, and step 211's `get` loop MUST read pending to its bound (exit 3). The provisioning run is a
#   stand-in that exits 0 and makes nothing (s01-standin-provision.cmd).
# - Home 2 holds a ready linux/amd64 host record at m8a.xlarge (state.md, the host registry): lease A at the default size and
#   lease B at --size s are both answered ready at once (D66), 212, 213, 220 and 230 read the real documents, and 202 and
#   250 read the real `lease list`. The operator, ssh and aws are still stand-ins; the CA is made by hand in the
#   home (Add-S01HandCa), since the operator's ca refuses a folder inside a git working tree.
param([int]$Base = 301)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-drafter-lib.ps1')
$dist = Join-Path $script:DRepo 'target\dist'
$exe = Join-Path $dist 'agentspace.exe'
$prov = Join-Path $script:DDir 's01-standin-provision.cmd'
$script:n = $Base
$script:bad = 0
$env:AGENTSPACE_CMD = $exe
$labs = @()

function Write-HomeFile($Lab, [string]$Rel, [string]$Text) {
  $p = Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $Lab.UserProfile $Rel)
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $p) | Out-Null
  Write-S01File $p $Text
}
function Enter-RealEnv($Lab) {
  $envs = Get-ConformanceChildEnvironment -ConformanceHome $Lab -Override @{ AGENTSPACE_PROVISION_CMD = $prov }
  foreach ($k in 'AGENTSPACE_STATE_DIR', 'AGENTSPACE_SSH_DIR', 'AGENTSPACE_PROXY_COMMAND', 'AGENTSPACE_PROVISION_CMD') { $script:DSaved[$k] = [Environment]::GetEnvironmentVariable($k); Set-DEnv $k ([string]$envs[$k]) }
}
function Step([string]$Case, $Lab, [string]$Step, [string]$What, [int]$Exit = 0, [string[]]$Must = @(), [string[]]$MustNot = @(), [hashtable]$EnvVars = @{}) {
  $name = ('{0:D3}-real-{1}-{2}' -f $script:n, $Step, $What); $script:n++
  $ev = @{ S01_AGENTSPACE_EXE = $exe; S01_AGENTSPACE = '' }
  foreach ($k in $EnvVars.Keys) { $ev[$k] = $EnvVars[$k] }
  $r = Invoke-S01Case -Lab $Lab -Case $Case -Name $name -Step $Step -ExpectExit $Exit -Must (@(@($(if ($Exit -eq 0) { 'PASS: every check of this step held.' })) | Where-Object { $_ }) + @($Must)) -MustNot $MustNot -EnvVars $ev
  if (-not $r.Ok) { $script:bad++ }
  return $r
}

try {
  # ---- Home 1: no host record, the project list approves amd64
  $lab1 = New-S01Lab
  $labs += $lab1
  Write-Host "Home 1 ready. The CA is made by hand: $(Add-S01HandCa -Lab $lab1)."
  if (-not $lab1.CaReady) { throw 'No throwaway CA could be made, so the real build cannot place a lease.' }
  Enter-S01Home -Lab $lab1
  Enter-RealEnv $lab1
  Write-HomeFile $lab1 '.agentspace\projects.json' ('{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20","platforms":[{"platform":"linux/amd64","approved":"2026-11-27"}]}]}' + "`n")
  $c1 = New-S01CaseDir 'real-1'
  Write-S01Observe -Case $c1 -OpenMinutes 20
  Initialize-S01Flow -Case $c1 -Leases none -Phase before
  Write-S01HomeLogs -Lab $lab1 -Provision $script:DProvisionOk -Guard @()
  [void](Step $c1 $lab1 '210' 'create-pending' 0 @("State: 'pending'", "ttl_seconds: '7200'", "size: 'm'", "platform: 'linux/amd64'", 'Schema lease.output.schema.json: True') $script:DNoLease)
  [void](Step $c1 $lab1 '211' 'get-pending-bound' 3 @('Pass 1 at', "state 'pending'", 'at the bound of 4 seconds') $script:DNoLease @{ S01_BOUND_SECONDS = '4' })
  Restore-S01Env
  Remove-ConformanceHome -ConformanceHome $lab1
  $labs = @()
  # ---- Home 2: a ready linux/amd64 host at m8a.xlarge
  $lab2 = New-S01Lab
  $labs += $lab2
  Write-Host "Home 2 ready. The CA is made by hand: $(Add-S01HandCa -Lab $lab2)."
  if (-not $lab2.CaReady) { throw 'No throwaway CA could be made for home 2.' }
  Enter-S01Home -Lab $lab2
  Enter-RealEnv $lab2
  Write-HomeFile $lab2 '.agentspace\hosts.json' ('{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"},{"project":"agentspace","platform":"linux/amd64","stack":"agentspace-linux-amd64-host-agentspace","state":"ready","instance_type":"m8a.xlarge"}]}' + "`n")
  $c2 = New-S01CaseDir 'real-2'
  Write-S01Observe -Case $c2 -OpenMinutes 20
  Initialize-S01Flow -Case $c2 -Leases none -Phase before
  Write-S01HomeLogs -Lab $lab2 -Provision $script:DProvisionOk -Guard @()
  # 202 needs the amd host absent from `status -All` (the stand-in's before phase) and the real, empty `lease list`.
  [void](Step $c2 $lab2 '202' 'list-before' 0 @('Total leases: 0; by state: none', 'Schema lease-list.output.schema.json: True'))
  # Lease A by the real build, as step 210 would make it if the host were ready: the driver, not a step, makes it.
  $r = Invoke-AgentspaceCli -ConformanceHome $lab2 -Arguments @('lease', 'create', '--platform', 'linux/amd64', '--project', 'agentspace', '--ttl', '2h')
  Write-Host "Driver's create of lease A: exit $($r.ExitCode); state '$(if ($r.Json) { $r.Json['state'] } else { 'no document' })'"
  if ($r.ExitCode -ne 0 -or -not $r.Json -or $r.Json['state'] -cne 'ready') { throw 'The real build did not answer ready for lease A on a ready host.' }
  Write-S01File (Join-Path $c2 'state\lease-A.id') ([string]$r.Json['id'])
  [void](Step $c2 $lab2 '212' 'get-ready' 0 @("State: 'ready'", "ssh.user: 'ubuntu'", 'ssh.host_alias present: True', 'ssh.host_alias is agentspace-<project>-<first 8 hex digits of id>: True', 'Schema lease.output.schema.json: True', "ttl_seconds: '7200'") $script:DNoLease)
  Write-S01File (Join-Path $c2 'standin\phase.txt') 'running'
  [void](Step $c2 $lab2 '213' 'ssh-real-alias' 0 @('Alias read from the state file: True (never printed)', 'Tools answering: 6 of 6') @('user ubuntu', 'proxycommand C:'))
  [void](Step $c2 $lab2 '220' 'create-b-ready' 0 @("size: 's'", "State: 'ready'", "Lease B's alias differs from lease A's: True", "Type 'm8a.xlarge'", 'ready: 2 (expected 2: A and B)') $script:DNoLease)
  [void](Step $c2 $lab2 '230' 'release' 0 @('Leases A and B listed as released: 2 of 2', 'as expected') $script:DNoLease)
  Write-S01File (Join-Path $c2 'standin\phase.txt') 'after'
  [void](Step $c2 $lab2 '250' 'list-after' 0 @('Total leases: 2; by state: released=2', 'Leases ready or pending, whole list: 0'))
} finally {
  Restore-S01Env
  foreach ($l in $labs) {
    $starts = Get-ConformanceTripwireStarts -ConformanceHome $l
    Remove-ConformanceHome -ConformanceHome $l
    Write-Host "Home removed: $(-not (Test-Path -LiteralPath $l.Root)); tripwire starts: $starts"
  }
}
Write-Host "Real-build dry run: $($script:n - $Base) steps run; not as designed: $($script:bad)"
if ($script:bad -eq 0) { exit 0 }
exit 1

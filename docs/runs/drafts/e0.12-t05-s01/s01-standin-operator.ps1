# The dry runs' stand-in for agentspace-operator.exe: status -All and status -Host agentspace -Platform linux/amd64, in the
# form docs\operator.md:3797-3824 prints, on stderr as the operator does. RFC 2119 keywords; the capitals are load-bearing.
# The phase is S01_STANDIN_DIR\phase.txt: before (no amd64 host, every host stopped), running (the amd64 host runs, type
# m8a.xlarge), after (the amd64 host is stopped). S01_PLANT names ONE planted answer. The identifiers are made up.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 's01-standin-lib.ps1')
$plant = Get-SlPlant
$phaseFile = Join-Path (Get-SlDir) 'phase.txt'
$phase = if (Test-Path -LiteralPath $phaseFile) { ([IO.File]::ReadAllText($phaseFile)).Trim() } else { 'before' }
$argv = @($args | ForEach-Object { [string]$_ })
if ($argv.Count -lt 1 -or $argv[0] -cne 'status') { [Console]::Error.WriteLine('agentspace: Only status is stood in.'); exit 1 }
$iid = 'i-' + '0a1b2c3d4e5f60718'

function New-SlHostText {
  param([string]$Name, [string]$Stack, [string]$Rec, [string]$Platform, [string]$State, [string]$Type, [string]$Ssm, [object]$Leases, [bool]$GuardLine = $false, [bool]$WithHostLine = $true)
  $t = @()
  if ($WithHostLine) { $t += "Host      $Name ($Stack), $Rec" }
  if ($Platform) { $t += "Platform  $Platform" }
  $t += "Instance  $iid"
  $t += "State     $State"
  $t += "Type      $Type"
  $t += ('Disk      20 GiB gp3 (vol' + '-0a1b2c3d4e5f60718)')
  $t += "SSM       $Ssm"
  if ($null -ne $Leases) { $t += "Leases    $Leases live" }
  if ($GuardLine) { $t += 'Guard     unexplained; stops at 9:41pm' }
  return $t
}

$amdState = if ($phase -ceq 'running') { 'running' } else { 'stopped' }

if ($argv -contains '-All') {
  if ($plant -ceq 'all-exit1') { [Console]::Error.WriteLine('agentspace: The AWS sign-in has expired.'); exit 1 }
  if ($plant -ceq 'all-empty') { exit 0 }
  $lines = @()
  $boxState = 'stopped'; $arm = 'stopped'
  if ($plant -ceq 'all-running-arm64') { $arm = 'running' }
  if ($plant -ceq 'all-box-pending') { $boxState = 'pending' }
  $leasesN = if ($plant -ceq 'all-no-leases-line') { $null } else { 0 }
  if ($plant -ceq 'all-leases-live') { $leasesN = 1 }
  if ($plant -cne 'all-missing-box') { $lines += New-SlHostText -Name 'swimcoach' -Stack 'agentbox' -Rec 'ready' -Platform '' -State $boxState -Type 'm9g.xlarge' -Ssm $(if ($boxState -ceq 'running') { 'online' } else { '-' }) -Leases $leasesN }
  $lines += New-SlHostText -Name 'agentspace' -Stack 'agentspace-host-agentspace' -Rec 'ready' -Platform '' -State $arm -Type 'm9g.xlarge' -Ssm $(if ($arm -ceq 'running') { 'online' } else { '-' }) -Leases $leasesN -GuardLine:($plant -ceq 'all-guard-line' -or $arm -ceq 'running')
  if ($phase -cne 'before' -and $plant -cne 'all-amd-absent') {
    $st = $amdState
    if ($plant -ceq 'all-amd-running') { $st = 'running' }
    $stack = if ($plant -ceq 'all-wrong-stack') { 'agentspace-host-agentspace' } else { 'agentspace-linux-amd64-host-agentspace' }
    $lines += New-SlHostText -Name 'agentspace.linux-amd64' -Stack $stack -Rec 'ready' -Platform 'linux/amd64' -State $st -Type 'm8a.xlarge' -Ssm $(if ($st -ceq 'running') { 'online' } else { '-' }) -Leases $leasesN -GuardLine:($st -ceq 'running' -and $plant -ceq 'all-amd-running')
  }
  if ($plant -ceq 'all-lease-file-note') { $lines += "agentspace: The lease state file X cannot be read: the file does not exist." }
  if ($plant -ceq 'all-amd-before') {
    $lines += New-SlHostText -Name 'agentspace.linux-amd64' -Stack 'agentspace-linux-amd64-host-agentspace' -Rec 'ready' -Platform 'linux/amd64' -State 'stopped' -Type 'm8a.xlarge' -Ssm '-' -Leases 0
  }
  foreach ($l in $lines) { [Console]::Error.WriteLine($l) }
  exit 0
}

# status -Host agentspace -Platform linux/amd64
if ($plant -ceq 'host-exit1') { [Console]::Error.WriteLine('agentspace: The AWS sign-in has expired.'); exit 1 }
$state = if ($plant -ceq 'host-stopped') { 'stopped' } else { 'running' }
$type = switch ($plant) { 'host-type' { 'm8a.large' } 'host-resized' { 'm8a.2xlarge' } default { 'm8a.xlarge' } }
$platform = if ($plant -ceq 'host-no-platform') { '' } else { 'linux/amd64' }
$ssm = if ($plant -ceq 'host-ssm') { 'not yet' } else { 'online' }
$lines = New-SlHostText -Name 'x' -Stack 'x' -Rec 'x' -Platform $platform -State $state -Type $type -Ssm $ssm -Leases $null -WithHostLine $false
if ($plant -ceq 'host-no-instance') { $lines = @($lines | Where-Object { $_ -cnotmatch '^Instance' }) }
foreach ($l in $lines) { [Console]::Error.WriteLine($l) }
exit 0

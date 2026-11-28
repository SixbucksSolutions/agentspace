# Stand-in for agentspace-operator.exe in E0.12-T05-S02's dry runs (drafter-owned). It answers `status -All`, `status -Host agentspace -Platform
# windows/amd64` and `sync -Host agentspace -Platform windows/amd64`, and refuses anything else: a command that names up, bootstrap, provision,
# grow, deploy or down, or the arm64 hosts, exits 2 and is logged as REFUSED.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
$line = $a -join ' '
if ($a.Count -ge 1 -and $a[0] -cin @('up', 'bootstrap', 'provision', 'grow', 'deploy', 'down')) { Stop-SiRefused 'operator' "the command $($a[0]) MUST NOT be sent by a step" }
if ($line -cmatch 'swimcoach|linux/arm64|-Host agentspace$') { Stop-SiRefused 'operator' 'a call that names an arm64 host' }
if ($line -ceq 'status -All') { Write-SiCall 'operator status -All'; Send-SiAnswer 'operator' 'status-all' }
if ($line -ceq 'status -Host agentspace -Platform windows/amd64') { Write-SiCall 'operator status -Host agentspace -Platform windows/amd64'; Send-SiAnswer 'operator' 'status-host' }
if ($line -ceq 'sync -Host agentspace -Platform windows/amd64') { Write-SiCall 'operator sync -Host agentspace -Platform windows/amd64'; Send-SiAnswer 'operator' 'sync' }
Stop-SiUnknown 'operator' $line

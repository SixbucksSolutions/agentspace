# Step 302 of E0.12-T05-S02: before the lease, `status -All` masked (every host stopped; agentspace.linux-amd64's state is
# printed, whatever S01's check left it at) and `lease list` counted by state. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing. The two arm64 hosts MUST be stopped: nothing here starts a host.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '302' 'before the lease: status -All masked, lease list counted')) { exit 5 }
exit (Invoke-S02Main { Invoke-S02StatusAndList -EveryHostStopped $false })

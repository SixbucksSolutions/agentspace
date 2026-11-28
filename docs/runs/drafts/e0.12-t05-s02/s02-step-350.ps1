# Step 350 of E0.12-T05-S02: at the close, `status -All` masked (every host stopped, agentspace.linux-amd64 and
# agentspace.windows-amd64 included) and `lease list` showing none ready or pending. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '350' 'at the close: every host stopped, no lease ready or pending')) { exit 5 }
exit (Invoke-S02Main { Invoke-S02StatusAndList -EveryHostStopped $true -RequireWindowsHost $true })

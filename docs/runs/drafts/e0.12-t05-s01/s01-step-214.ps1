# Step 214: status -Host agentspace -Platform linux/amd64, through the mask: running, m8a.xlarge. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '214' "status of $($script:S01HostName): State running, Type $($script:S01HostType)"
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    Test-S01HostStatus -Problems $p
    return (Complete-S01 $p)
  })

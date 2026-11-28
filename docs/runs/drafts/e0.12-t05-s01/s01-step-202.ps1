# Step 202: status -All before, through the mask, and lease list counted by state. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. Every host listed MUST read stopped; agentspace's linux/arm64 host and
# agentbox are named and MUST be listed; an amd64 host is absent or stopped; no lease is ready or pending.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '202' 'every host stopped, no lease ready or pending, before the check'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    Test-S01StatusAll -Phase 'before' -Problems $p
    $l = Read-S01LeaseList -Problems $p
    if ($null -ne $l -and $l.Hot -ne 0) { $p.Add("$($l.Hot) lease(s) are ready or pending anywhere in the list; a host may be running") }
    # guard.log's count, kept as a baseline for the whole run (step 230 keeps the one step 240 reads).
    $n = Get-S01GuardCount
    Set-S01State 'guard-202.count' ([string]$n)
    Write-S01 "guard.log lines now: $n (kept as the run's first baseline)"
    return (Complete-S01 $p)
  })

# Step 230: release lease B, then lease A (both attempted whatever the first answers), then lease list counted.
# Run through scripts\live\wrap.ps1. RFC 2119 keywords; the capitals are load-bearing.
# Before the releases it keeps guard.log's line count (guard-230.count), the baseline step 240 reads from; after them, the
# end time in UTC (end-230.utc), which step 240's 35-minute bound and its 25-minute check count from. Neither is printed raw.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '230' 'release lease B, then lease A; none of the check''s leases is left ready'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $a = Get-S01LeaseId 'A'
    $b = Get-S01LeaseId 'B'
    if (-not $a -and -not $b) { Write-S01 'FINDING: The state file holds no lease; there is nothing to release.'; return 1 }
    if (-not $a) { $p.Add('the state file holds no lease A') }
    if (-not $b) { $p.Add('the state file holds no lease B') }
    $n = Get-S01GuardCount
    Set-S01State 'guard-230.count' ([string]$n)
    Write-S01 "guard.log lines before the releases: $n (the baseline for step 240)"
    $sizes = @{ A = 'm'; B = 's' }
    foreach ($which in @('B', 'A')) {
      $id = if ($which -ceq 'B') { $b } else { $a }
      if (-not $id) { continue }
      Write-S01 "Command: agentspace lease release <lease $which>"
      $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'release', $id) -TimeoutSec 180
      $before = $p.Count
      $want = @{ State = 'released'; Size = $sizes[$which]; Members = @('id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'expires_at', 'released_at'); User = $null; Ttl = $script:S01TtlSeconds; Expires = 'any'; Id = $id; Released = $true }
      $sub = New-S01List
      [void](Test-S01Lease -Run $run -Want $want -Problems $sub -StepName '230')
      foreach ($x in $sub) { $p.Add("lease ${which}: $x") }
      Write-S01 "Release of lease $which finished at $(Get-S01Clock): $(if ($p.Count -eq $before) { 'as expected' } else { 'NOT as expected' })"
    }
    $end = [DateTimeOffset]::UtcNow
    Set-S01State 'end-230.utc' $end.ToString('yyyy-MM-ddTHH:mm:ssZ', $script:S01Inv)
    $by = $end.AddMinutes(25)
    Write-S01 "The releases ended at $(ConvertTo-S01Local $end); D56's bound is 25 minutes, so the guard stops the host by $(ConvertTo-S01Local $by) (step 240 waits to $(ConvertTo-S01Local $end.AddMinutes(35)))."
    $l = Read-S01LeaseList -Problems $p
    if ($null -ne $l) {
      if ($l.OursHot.Count -ne 0) { $p.Add("$($l.OursHot.Count) lease(s) of the project on $($script:S01Platform) are still ready or pending") }
      $released = @($l.Leases | Where-Object { ($_['id'] -ceq $a -or $_['id'] -ceq $b) -and $_['state'] -ceq 'released' })
      Write-S01 "Leases A and B listed as released: $($released.Count) of 2"
      if ($released.Count -ne 2) { $p.Add("$($released.Count) of the check's 2 leases are listed as released") }
    }
    return (Complete-S01 $p)
  })

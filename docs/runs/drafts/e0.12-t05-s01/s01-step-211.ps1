# Step 211: the wait. `lease get` once a minute, at most 50 minutes, until lease A is ready. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing.
# Exit 0 ready; 2 unsatisfiable, expired, released or any other state but pending and ready; 3 the bound; 6 the observer's
# hours; 1 a failed read, or a `sync failed` line seen in provision.log (README.md:640's note on #0163 is stale: the card is
# fixed, D61, so such a line is a finding, and the loop's own `lease get` is what starts the next run, as that note asks).
# Each pass prints its time, the state, and the masked last 3 lines of hosts\agentspace.linux-amd64\provision.log.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '211' 'wait for lease A to become ready (lease get once a minute, at most 50 minutes)'
    if ($null -ne $stop) { return $stop }
    $id = Get-S01LeaseId 'A'
    if (-not $id) { Write-S01 'FINDING: The state file holds no lease A; step 210 has not run.'; return 1 }
    $poll = if ($env:S01_POLL_SECONDS) { [int]$env:S01_POLL_SECONDS } else { 60 }
    $bound = if ($env:S01_BOUND_SECONDS) { [int]$env:S01_BOUND_SECONDS } else { 3000 }
    Write-S01 "Bound: $bound seconds; a read every $poll seconds."
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $pass = 0
    $syncFailed = 0
    $seenFailLines = 0
    $logPath = Get-S01ProvisionLogPath
    while ($true) {
      $pass++
      $h = Test-S01Hours "Pass $pass"
      if ($null -ne $h) { return $h }
      $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'get', $id) -TimeoutSec 120
      $doc = $null
      if (-not $run.StartFailed -and -not $run.TimedOut -and $run.ExitCode -eq 0) { $doc = ConvertFrom-S01Json $run.Stdout.Trim() }
      if ($null -eq $doc -or $doc -isnot [hashtable]) {
        Write-S01 "Pass ${pass} at $(Get-S01Clock): lease get failed (exit $($run.ExitCode), started $(-not $run.StartFailed), timed out $($run.TimedOut))."
        if ($run.StderrBytes.Length -gt 0) { Write-S01 ('Error document (masked): ' + (Get-S01Excerpt $run)) }
        Write-S01 'FINDING: lease get did not print a lease; the wait ends here.'
        return 1
      }
      $state = [string]$doc['state']
      $lines = Read-S01LogLines $logPath
      Write-S01 "Pass ${pass} at $(Get-S01Clock), $([int]$sw.Elapsed.TotalSeconds) s in: lease get exit 0, state '$state'."
      if ($null -eq $lines) { Write-S01 '  provision.log: not there yet' }
      else {
        Write-S01 "  provision.log: $($lines.Count) lines; the last 3, masked:"
        foreach ($l in @($lines | Select-Object -Last 3)) { $t = $l; if ($t.Length -gt 220) { $t = $t.Substring(0, 220) + '...' }; Write-S01 ('  | ' + $t) }
        $failLines = @($lines | Where-Object { $_ -cmatch '(?<!patch )\b(sync|run|record|place|provision) failed\b' })
        $sf = @($lines | Where-Object { $_ -cmatch '\bsync failed\b' })
        if ($failLines.Count -gt $seenFailLines) { Write-S01 "  NOTE: $($failLines.Count - $seenFailLines) new failure line(s) in provision.log, $($sf.Count) of them 'sync failed'."; $seenFailLines = $failLines.Count }
        $syncFailed = $sf.Count
        if (@($lines | Where-Object { $_ -cmatch 'patch run failed' }).Count -gt 0) { Write-S01 '  NOTE: provision.log says the patch run failed; sync goes on without it (operator.md, The patch wait).' }
      }
      if ($state -ceq 'ready') {
        Write-S01 "Lease A is ready after $pass read(s), $([int]$sw.Elapsed.TotalSeconds) seconds, at $(Get-S01Clock)."
        Set-S01State 'ready-seconds.txt' ([string][int]$sw.Elapsed.TotalSeconds)
        if ($seenFailLines -gt 0) {
          Write-S01 "FINDING: provision.log holds $seenFailLines failure line(s), $syncFailed of them 'sync failed', on the way to ready. #0163 is fixed (D61; docs\runs\e0.08-t05-s01.md Part A), so a first run MUST NOT fail at sync."
          return 1
        }
        Write-S01 'Lease A is ready, and provision.log shows no failed run.'
        return (Complete-S01 (New-S01List))
      }
      if ($state -cne 'pending') {
        Write-S01 "FINDING: lease A is '$state', not pending or ready; it will not become ready. The runner stops and reports."
        return 2
      }
      if ($sw.Elapsed.TotalSeconds + $poll -gt $bound) {
        Write-S01 "FINDING: lease A is still pending after $([int]$sw.Elapsed.TotalSeconds) seconds, at the bound of $bound seconds; no read is made past it."
        return 3
      }
      Start-Sleep -Seconds $poll
    }
  })

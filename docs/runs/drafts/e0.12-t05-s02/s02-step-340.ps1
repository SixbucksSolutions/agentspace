# Step 340 of E0.12-T05-S02: Release W (`agentspace lease release <id>`), the answer `released`, and `lease list` counted. Run through
# scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# - Before the release it counts guard.log's lines (guard-baseline.txt), so step 341 reads only the lines the guard writes after the release.
#   A baseline is a count taken here, never "absent" (architect.md, Live checks, v0.09-T05).
# - After it, it writes the release's end time (UTC) to release-end.txt, which step 341 counts the guard's bound from.
# - Step 340 MUST run only after Terry's sessions are closed: a connection open at the release is ended by the release (docs/lease-api.md, release).
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '340' 'Release W; the answer released; lease list counted')) { exit 5 }
if (-not (Test-S02State 'lease-w.id')) { Stop-S02Step 'no lease W state file; step 310 has not run.'; exit 5 }
if (Test-S02State 'release-end.txt') { Stop-S02Step 'a release time is recorded already (release-end.txt); lease W was released by an earlier run of this step.'; exit 5 }
exit (Invoke-S02Main {
    $id = Read-S02State 'lease-w.id'
    $guardLines = @(Read-S02LogLines (Get-S02GuardLogPath)).Count
    Write-S02State 'guard-baseline.txt' ([string]$guardLines + "`n")
    Write-S02Line "guard.log lines before the release: $guardLines"
    $r = Invoke-S02Tool -Tool 'agentspace' -Arguments @('lease', 'release', $id) -TimeoutSeconds 240
    $ended = $r.EndedUtc
    Write-S02Line "release exit: $($r.ExitCode); elapsed $([int]$r.ElapsedSeconds) s; ended $(Get-S02LocalText $ended)"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'release exited 0.' "release exited $($r.ExitCode)."
    $doc = ConvertFrom-S02Json $r.Stdout.TrimEnd("`n")
    if ($null -eq $doc -or $doc -isnot [System.Collections.IDictionary]) { Add-S02Finding 'release did not print a JSON object.'; return }
    Write-S02Line "Members: $((Get-S02SortedKeys $doc) -join ','); state '$($doc['state'])'; platform '$($doc['platform'])'; released_at $(if ($doc['released_at'] -is [string]) { Format-S02Time $doc['released_at'] } else { 'absent' })"
    Assert-S02 ($doc['state'] -ceq 'released') 'The answer is released.' "The answer's state is '$($doc['state'])', expected 'released'."
    if ($r.ExitCode -eq 0 -and $doc['state'] -ceq 'released') {
      Write-S02State 'release-end.txt' ($ended.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) + "`n")
      Write-S02Line 'The release''s end time is recorded for step 341.'
    }
    Assert-S02 ($doc['released_at'] -is [string]) 'The answer has released_at.' 'The answer has no released_at.'
    Assert-S02 (-not $doc.ContainsKey('ssh')) 'The answer has no ssh object.' 'The released lease still has an ssh object.'
    Assert-S02 ($doc['platform'] -ceq 'windows/amd64') 'The platform is windows/amd64.' "The platform is '$($doc['platform'])'."
    $ll = Read-S02LeaseList
    if ($null -ne $ll) {
      Write-S02Line "Total leases: $($ll.Total)"
      Write-S02Line ('Leases by state: ' + (Format-S02Counts $ll.ByState))
      Write-S02Line ('windows/amd64 leases by state: ' + (Format-S02Counts $ll.WindowsByState))
      $hot = 0
      foreach ($s in 'ready', 'pending') { if ($ll.ByState.ContainsKey($s)) { $hot += $ll.ByState[$s] } }
      Assert-S02 ($hot -eq 0) 'No lease is ready or pending.' "$hot lease(s) are ready or pending after the release."
    }
  })

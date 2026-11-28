# Step 250: status -All after: every host stopped, no lease ready or pending. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. The check's own host MUST be listed (made by it) and stopped.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '250' 'every host stopped, no lease ready or pending, after the check'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    Test-S01StatusAll -Phase 'after' -Problems $p
    $l = Read-S01LeaseList -Problems $p
    if ($null -ne $l -and $l.Hot -ne 0) { $p.Add("$($l.Hot) lease(s) are ready or pending anywhere in the list") }
    $h = Test-S01Hours 'At the close'
    if ($null -ne $h) { return $h }
    return (Complete-S01 $p)
  })

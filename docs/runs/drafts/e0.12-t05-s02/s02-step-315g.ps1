# Step 315g of E0.12-T05-S02, measure (g): no restart that Windows Update schedules by itself, from the System event log over the host's
# uptime: event IDs 1074, 6005, 6006, 6008 and 41, and the WindowsUpdateClient source, as counts and times only. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
#
# Over THIS boot's window: 6005 (the event log started) once; 1074 (a restart or shutdown was initiated), 6006 (a clean shutdown), 6008 (an
# unexpected shutdown) and 41 (Kernel-Power) none. A 1074 is the patch run's own `shutdown /r` or a restart Windows scheduled; this window
# holds neither, since a restart ends the boot it ran in. The WindowsUpdateClient source's counts are printed, not judged.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315g' 'measure (g): no restart scheduled by Windows Update, from the System event log over the uptime')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $h = Invoke-S02HostScript -Alias $alias -Name '315g' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    Write-S02Line ('Window: from the host boot ' + (Format-S02Time (Get-S02First $q 'boot_utc')) + ' to ' + (Format-S02Time (Get-S02First $q 'now_utc')))
    $want = @{ '1074' = 0; '6005' = 1; '6006' = 0; '6008' = 0; '41' = 0 }
    $seen = @{}
    if ($q.By.ContainsKey('sysevent')) {
      foreach ($v in $q.By['sysevent']) {
        $f = $v -split '\|', 3
        $seen[$f[0]] = [int]$f[1]
        $times = if ($f.Count -ge 3 -and $f[2] -ne '') { ($f[2] -split ';' | ForEach-Object { Format-S02Time $_ }) -join '; ' } else { 'none' }
        Write-S02Line "event $($f[0]): $($f[1]) time(s); $times"
      }
    }
    foreach ($id in '1074', '6005', '6006', '6008', '41') {
      Assert-S02 ($seen.ContainsKey($id)) "Event $id was read." "Event $id was not read."
      if ($seen.ContainsKey($id)) { Assert-S02 ($seen[$id] -eq $want[$id]) "Event $id count is $($want[$id])." "Event $id count is $($seen[$id]), expected $($want[$id]) in this boot's window." }
    }
    Write-S02Line "1074 events in the log's whole history (information): $(Get-S02First $q 'sysevent_1074_all_time')"
    Write-S02Line "WindowsUpdateClient events since boot: $(Get-S02First $q 'wuclient_events')"
    if ($q.By.ContainsKey('wuclient')) {
      foreach ($v in $q.By['wuclient']) { $f = $v -split '\|', 4; Write-S02Line "  WindowsUpdateClient event $($f[0]): $($f[1]) time(s), first $(Format-S02Time $f[2]), last $(Format-S02Time $f[3])" }
    }
    Assert-S02 ($null -ne (Get-S02First $q 'wuclient_events')) 'The WindowsUpdateClient source was read.' 'The WindowsUpdateClient source was not read.'
  })

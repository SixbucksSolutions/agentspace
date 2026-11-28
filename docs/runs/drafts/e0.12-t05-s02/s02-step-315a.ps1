# Step 315a of E0.12-T05-S02, measure (a): the patch run's time, from patch.log's own times, against the patch task's 20-minute limit and
# `up`'s 15-minute wait (operator.md, The patch run and its restart rule; Starting the box and waiting for it). Also card addition (i)'s count of
# `Tools:` lines per patch run. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
# The time is measured from the log's `Patch run started` line to its `Patch run finished` line, so it holds the tools step.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315a' 'measure (a): the patch run''s time against the task''s 20 minutes and up''s 15')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $h = Invoke-S02HostScript -Alias $alias -Name '315a' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    Write-S02Line ('Host boot: ' + (Format-S02Time (Get-S02First $q 'boot_utc')) + '; now: ' + (Format-S02Time (Get-S02First $q 'now_utc')))
    Write-S02Line "patch.log lines: $(Get-S02First $q 'patch_log_lines')"
    Write-S02Line "patched marker: $(Get-S02First $q 'marker_patched'); patch-failed marker: $(Get-S02First $q 'marker_patch-failed')"
    Write-S02Line "patch task last result: $(Get-S02First $q 'patch_task_last_result'); last run: $(Format-S02Time (Get-S02First $q 'patch_task_last_run_utc'))"
    $boot = ConvertFrom-S02Utc (Get-S02First $q 'boot_utc')
    $runs = @()
    if ($q.By.ContainsKey('run')) {
      foreach ($v in $q.By['run']) {
        $f = $v -split '\|', 8
        if ($f.Count -lt 8) { continue }
        $runs += [pscustomobject]@{ N = [int]$f[0]; Start = $f[1]; End = $f[2]; Seconds = $f[3]; Result = $f[4]; Tools = $f[5]; Summary = $f[6]; Search = $f[7] }
      }
    }
    Write-S02Line "patch runs in the log: $($runs.Count)"
    foreach ($r in $runs) {
      Write-S02Line ("run $($r.N): started " + (Format-S02Time $r.Start) + "; ended " + $(if ($r.End) { Format-S02Time $r.End } else { 'never' }) + "; $($r.Seconds) s; $($r.Result); $($r.Tools); $($r.Summary); '$($r.Search)'")
    }
    if ($q.By.ContainsKey('tools_line')) { foreach ($v in $q.By['tools_line']) { Write-S02Line ('Tools: line: ' + (Format-S02LogLine $v)) } }
    Assert-S02 ($runs.Count -ge 1) 'patch.log holds at least one patch run.' 'patch.log holds no patch run.'
    $thisBoot = @($runs | Where-Object { $null -ne $boot -and $null -ne (ConvertFrom-S02Utc $_.Start) -and (ConvertFrom-S02Utc $_.Start) -ge $boot })
    Assert-S02 ($thisBoot.Count -ge 1) 'A patch run started this boot.' 'No patch run started since this boot.'
    foreach ($r in $thisBoot) {
      Assert-S02 ($r.Result -ceq 'finished') "Run $($r.N) finished." "Run $($r.N) is '$($r.Result)', not finished."
      if ($r.Result -ceq 'finished' -and $r.Seconds -match '^\d+$') {
        $s = [int]$r.Seconds
        Write-S02Line "Run $($r.N) took $s s ($([Math]::Round($s / 60.0, 1)) min); up's wait is 900 s; the task's limit is 1200 s."
        Assert-S02 ($s -le 900) "Run $($r.N) is inside up's 15 minutes." "Run $($r.N) took $s s, past up's 15-minute wait$(if ($s -gt 1200) { ' and past the task''s 20-minute limit' }); the measurement goes back to the architect (operator.md, The patch check)."
        $sum = [int](($r.Summary -split '=')[1])
        $tl = [int](($r.Tools -split '=')[1])
        Write-S02Line "Run $($r.N): $tl Tools: line(s), of which $sum are the version summary."
        Assert-S02 ($sum -eq 1) "Run $($r.N) holds exactly one Tools: version-summary line." "Run $($r.N) holds $sum Tools: version-summary lines, expected 1."
      }
    }
    Assert-S02 ((Get-S02First $q 'marker_patched') -cmatch '^written ') 'The patched marker exists.' 'The patched marker is missing.'
    Assert-S02 ((Get-S02First $q 'marker_patch-failed') -ceq 'missing') 'No patch-failed marker.' 'A patch-failed marker exists.'
    Assert-S02 ((Get-S02First $q 'patch_task_last_result') -ceq '0') 'The patch task''s last result is 0.' "The patch task's last result is '$(Get-S02First $q 'patch_task_last_result')'."
  })

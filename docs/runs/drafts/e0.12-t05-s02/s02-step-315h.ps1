# Step 315h of E0.12-T05-S02, measure (h): sync's patch wait on a new host's first boot answered within its wait, not at its 30-minute limit
# (operator.md, sync on a Windows host; pin P18). From provision.log's own times and patch.log's run on the host: `sync done` has no
# "the patch run failed" text, the patch run finished no later than `sync done`, and `sync done` came less than 30 minutes after the patch run
# began. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315h' 'measure (h): sync''s patch wait answered within its wait, not at its 30-minute limit')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $baseText = Read-S02State 'provlog-baseline.txt'
    $base = if ($baseText -match '^\d+$') { [int]$baseText } else { 0 }
    $all = @(Read-S02LogLines (Get-S02ProvisionLogPath))
    $log = @(if ($all.Count -gt $base) { $all[$base..($all.Count - 1)] })
    $sync = @($log | Where-Object { $_ -cmatch '^\S+ sync (done|failed)' } | Select-Object -First 1)
    Assert-S02 ($sync.Count -eq 1) 'provision.log shows this run''s sync line.' 'provision.log shows no sync line since the baseline.'
    if ($sync.Count -ne 1) { return }
    Write-S02Line ('log: ' + (Format-S02LogLine $sync[0]))
    Assert-S02 ($sync[0] -cmatch '^\S+ sync done$') 'sync done carries no "the patch run failed" text.' 'sync did not end with a plain "sync done".'
    $syncAt = ConvertFrom-S02Utc ($sync[0].Split(' ')[0])
    $h = Invoke-S02HostScript -Alias $alias -Name '315a' -Quiet
    if ($null -eq $h) { return }
    $runs = @()
    if ($h.Probe.By.ContainsKey('run')) {
      foreach ($v in $h.Probe.By['run']) { $f = $v -split '\|', 8; if ($f.Count -ge 5) { $runs += [pscustomobject]@{ N = $f[0]; Start = $f[1]; End = $f[2]; Result = $f[4] } } }
    }
    $first = @($runs | Select-Object -First 1)
    Assert-S02 ($first.Count -eq 1) 'patch.log holds the first patch run.' 'patch.log holds no patch run.'
    if ($first.Count -ne 1) { return }
    $r = $first[0]
    $started = ConvertFrom-S02Utc $r.Start
    $ended = ConvertFrom-S02Utc $r.End
    Write-S02Line ("First patch run: started $(Format-S02Time $r.Start); ended $(if ($r.End) { Format-S02Time $r.End } else { 'never' }); $($r.Result)")
    Assert-S02 ($null -ne $ended -and $r.Result -ceq 'finished') 'The first patch run finished.' "The first patch run is '$($r.Result)'."
    if ($null -ne $ended -and $null -ne $syncAt -and $null -ne $started) {
      $slack = [int][Math]::Round(($syncAt - $ended).TotalSeconds)
      $span = [int][Math]::Round(($syncAt - $started).TotalSeconds)
      Write-S02Line "sync done came $slack s after the patch run ended, and $span s after it began (the wait's limit is 1800 s)."
      Assert-S02 ($slack -ge -5) 'The patch run had ended when sync moved on: the wait was answered.' "sync done came $([Math]::Abs($slack)) s BEFORE the patch run ended: sync did not wait for it."
      Assert-S02 ($span -lt 1770) 'sync moved on well inside the 30-minute limit.' "sync done came $span s after the patch run began: at the wait's 30-minute limit."
    } else { Add-S02Finding 'A time of the patch run or of sync done could not be read.' }
  })

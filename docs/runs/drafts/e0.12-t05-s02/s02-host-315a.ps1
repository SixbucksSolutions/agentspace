# Host-side probe of measures (a), (h) and (i)'s Tools: count (runs ON the Windows host under PowerShell 7, as agent; sent by scp and
# removed by itself): patch.log's runs with their own times, the Tools: lines of each, this boot's markers, and the patch task's last
# run. Prints `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function U([DateTime]$T) { return $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
$dir = 'C:\ProgramData\agentspace'
$boot = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
P 'boot_utc' (U $boot)
P 'now_utc' (U (Get-Date))
foreach ($m in 'patched', 'patch-failed') {
    $f = Join-Path $dir $m
    if (Test-Path -LiteralPath $f -PathType Leaf) { P ('marker_' + $m) ('written ' + (U (Get-Item -LiteralPath $f -Force).LastWriteTimeUtc)) } else { P ('marker_' + $m) 'missing' }
}
try {
    $info = Get-ScheduledTaskInfo -TaskPath '\agentspace\' -TaskName 'agentspace-patch' -ErrorAction Stop
    P 'patch_task_last_run_utc' (U $info.LastRunTime)
    P 'patch_task_last_result' $info.LastTaskResult
} catch { P 'patch_task_last_run_utc' 'unreadable' }

$log = Join-Path $dir 'patch.log'
if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { P 'patch_log_lines' 'missing'; Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue; exit 0 }
$lines = @([IO.File]::ReadAllLines($log) | Where-Object { $_ -ne '' })
P 'patch_log_lines' $lines.Count
$runs = New-Object System.Collections.ArrayList
$cur = $null
foreach ($l in $lines) {
    $m = [regex]::Match($l, '^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (.*)$')
    if (-not $m.Success) { continue }
    $t = $m.Groups[1].Value
    $text = $m.Groups[2].Value
    if ($text -ceq 'Patch run started') {
        if ($null -ne $cur) { $cur.result = 'incomplete'; [void]$runs.Add($cur) }
        $cur = @{ start = $t; end = ''; result = ''; tools = 0; summary = 0; search = '' }
        continue
    }
    if ($null -eq $cur) { continue }
    if ($text -cmatch '^Tools: ') { $cur.tools++; if ($text -cmatch '^Tools: Node ') { $cur.summary++ }; P 'tools_line' ($t + ' ' + $text) }
    elseif ($text -cmatch '^Search result code') { $cur.search = $text }
    elseif ($text -ceq 'Patch run finished') { $cur.end = $t; $cur.result = 'finished'; [void]$runs.Add($cur); $cur = $null }
    elseif ($text -cmatch '^Patch run failed') { $cur.end = $t; $cur.result = 'failed'; [void]$runs.Add($cur); $cur = $null }
}
if ($null -ne $cur) { $cur.result = 'incomplete'; [void]$runs.Add($cur) }
$i = 0
foreach ($r in $runs) {
    $i++
    $secs = 'none'
    if ($r.end -ne '') { $secs = [int]([DateTime]::Parse($r.end).ToUniversalTime() - [DateTime]::Parse($r.start).ToUniversalTime()).TotalSeconds }
    P 'run' ("$i|$($r.start)|$($r.end)|$secs|$($r.result)|tools=$($r.tools)|summary=$($r.summary)|$($r.search)")
}
P 'runs' $runs.Count
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0

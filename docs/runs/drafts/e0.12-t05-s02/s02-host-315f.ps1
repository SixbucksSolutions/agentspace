# Host-side probe of measure (f) and of the idle task's trigger (runs ON the Windows host under PowerShell 7, as agent; sent by scp and
# removed by itself): every line of idle.log since this boot, and the idle task's trigger, last and next run times. Prints
# `S02 key=value` lines. RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Continue'
function P([string]$Key, $Value) { [Console]::Out.WriteLine('S02 ' + $Key + '=' + $Value) }
function U($T) { if ($null -eq $T) { return 'none' }; return ([DateTime]$T).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
$boot = (Get-CimInstance -ClassName Win32_OperatingSystem).LastBootUpTime
$now = Get-Date
P 'boot_utc' (U $boot)
P 'now_utc' (U $now)
P 'uptime_seconds' ([int]($now - $boot).TotalSeconds)

$log = 'C:\ProgramData\agentspace\idle.log'
$kept = @()
if (Test-Path -LiteralPath $log -PathType Leaf) {
    foreach ($l in [IO.File]::ReadAllLines($log)) {
        $m = [regex]::Match($l, '^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (.*)$')
        if (-not $m.Success) { continue }
        if ([DateTime]::Parse($m.Groups[1].Value).ToUniversalTime() -ge $boot.ToUniversalTime()) { $kept += $l }
    }
    P 'idle_log' 'present'
} else {
    P 'idle_log' 'missing'
}
P 'idle_lines_since_boot' $kept.Count
foreach ($l in ($kept | Select-Object -Last 80)) { P 'idle' $l }

try {
    $task = Get-ScheduledTask -TaskPath '\agentspace\' -TaskName 'agentspace-idle' -ErrorAction Stop
    $info = Get-ScheduledTaskInfo -TaskPath '\agentspace\' -TaskName 'agentspace-idle' -ErrorAction Stop
    P 'idle_task_state' $task.State
    P 'idle_task_last_run_utc' (U $info.LastRunTime)
    P 'idle_task_next_run_utc' (U $info.NextRunTime)
    P 'idle_task_last_result' $info.LastTaskResult
    P 'idle_task_missed_runs' $info.NumberOfMissedRuns
    $t = $task.Triggers[0]
    P 'idle_trigger_class' $t.CimClass.CimClassName
    P 'idle_trigger_delay' $t.Delay
    P 'idle_trigger_interval' $t.Repetition.Interval
    P 'idle_trigger_duration' $t.Repetition.Duration
    P 'idle_trigger_stop_at_end' $t.Repetition.StopAtDurationEnd
    try { P 'idle_task_registered_utc' (U ([DateTime]$task.Date)) } catch { P 'idle_task_registered_utc' 'unknown' }
} catch {
    P 'idle_task_state' ('unreadable:' + $_.Exception.GetType().Name)
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
exit 0

# Step 315f of E0.12-T05-S02, measure (f): the idle check's lines with a lease held and the host idle, against 6 CPU-seconds per 5-minute
# check (operator.md, The idle check on a Windows host: "Windows' own background work, at 6 CPU-seconds per 5-minute check, is unmeasured").
# It first waits S02_IDLE_SECONDS (default 720, so two or three checks fall in it) with NO connection to the host from the laptop, checking the
# observer's hours each minute, then reads idle.log's lines since this boot through one host script. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
#
# The idle check's lines (box/windows/agentspace-idle-check.ps1, as operator.md gives them): `Active: SSH session open`, `Active: Session
# Manager session open`, `Active: Patching`, `Active: CPU work: <s> s since last check`, `Idle for <n> min of 60 (CPU <s> s since last check)`.
# An "idle-window check" is an `Idle for` line, which the check writes only with no session, no patch run and CPU under the limit. A held
# lease's keeper holder may keep an SSH connection open, which would make every check `Active: SSH session open`: the counts say so.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315f' 'measure (f): the idle check''s lines with the lease held and the host idle')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $wait = Get-S02Knob 'S02_IDLE_SECONDS' 720
    $until = [DateTime]::UtcNow.AddSeconds($wait)
    Write-S02Line "Waiting $wait s with no connection to the host (until $(Get-S02LocalText $until))."
    while ([DateTime]::UtcNow -lt $until) {
      if (Test-S02HoursReached) { Set-S02Exit 6; return }
      $left = [int][Math]::Ceiling(($until - [DateTime]::UtcNow).TotalSeconds)
      Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min(60, $left)))
    }
    $h = Invoke-S02HostScript -Alias $alias -Name '315f' -Quiet
    if ($null -eq $h) { return }
    $q = $h.Probe
    Write-S02Line ('Host boot: ' + (Format-S02Time (Get-S02First $q 'boot_utc')) + '; now: ' + (Format-S02Time (Get-S02First $q 'now_utc')) + "; uptime $(Get-S02First $q 'uptime_seconds') s")
    $count = Get-S02First $q 'idle_lines_since_boot'
    Write-S02Line "idle.log: $(Get-S02First $q 'idle_log'); lines since boot: $count"
    $idleLines = @(if ($q.By.ContainsKey('idle')) { $q.By['idle'] })
    foreach ($l in $idleLines) { Write-S02Line ('idle.log: ' + (Format-S02LogLine $l)) }
    Write-S02Line ("idle task: state $(Get-S02First $q 'idle_task_state'); last run $(Format-S02Time (Get-S02First $q 'idle_task_last_run_utc')); next run $(Format-S02Time (Get-S02First $q 'idle_task_next_run_utc')); last result $(Get-S02First $q 'idle_task_last_result')")
    Assert-S02 ($idleLines.Count -ge 1) 'The idle check has written at least one line since this boot.' 'idle.log has no line since this boot: the idle check has not run (see the task''s last and next run above).'
    $kinds = @{ ssh = 0; sm = 0; patch = 0; cpu = 0; idle = 0; other = 0 }
    $idleCpu = [System.Collections.Generic.List[int]]::new()
    $busy = [System.Collections.Generic.List[int]]::new()
    foreach ($l in $idleLines) {
      $text = ($l -replace '^\S+ ', '')
      if ($text -ceq 'Active: SSH session open') { $kinds['ssh']++ }
      elseif ($text -ceq 'Active: Session Manager session open') { $kinds['sm']++ }
      elseif ($text -ceq 'Active: Patching') { $kinds['patch']++ }
      elseif ($text -cmatch '^Active: CPU work: (\d+) s since last check$') { $kinds['cpu']++; $busy.Add([int]$Matches[1]) }
      elseif ($text -cmatch '^Idle for \d+ min of \d+ \(CPU (\d+) s since last check\)$') { $kinds['idle']++; $idleCpu.Add([int]$Matches[1]) }
      else { $kinds['other']++ }
    }
    Write-S02Line ("Checks by kind: SSH session open $($kinds['ssh']); Session Manager session open $($kinds['sm']); Patching $($kinds['patch']); CPU work $($kinds['cpu']); Idle for $($kinds['idle']); other $($kinds['other'])")
    if ($idleCpu.Count -gt 0) { Write-S02Line ('CPU seconds of the idle-window checks: ' + ($idleCpu -join ', ') + "; the most: $(($idleCpu | Measure-Object -Maximum).Maximum); the limit is 6") }
    if ($busy.Count -gt 0) { Write-S02Line ('CPU seconds of the checks that counted as active for CPU: ' + ($busy -join ', ')) }
    Assert-S02 ($kinds['other'] -eq 0) 'Every line is one of the idle check''s forms.' "$($kinds['other']) line(s) are none of the idle check's forms."
    Assert-S02 ($idleCpu.Count -ge 2) 'At least two idle-window checks were observed.' "Only $($idleCpu.Count) idle-window check(s) were observed ($($kinds['ssh']) saw an SSH session open, $($kinds['sm']) a Session Manager session, $($kinds['patch']) a patch run): the host was not idle for the check, so the measure is not made."
    Assert-S02 ($kinds['cpu'] -eq 0) 'No quiet check counted as active for CPU.' "$($kinds['cpu']) check(s) with no session counted as active for CPU work (6 s or more since the last check): Windows' own background work reaches the limit."
    if ($idleCpu.Count -gt 0) { Assert-S02 ((($idleCpu | Measure-Object -Maximum).Maximum) -lt 6) 'Every idle-window check is under 6 CPU-seconds.' 'An idle-window check reached 6 CPU-seconds.' }
  })

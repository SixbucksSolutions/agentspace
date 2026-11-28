# Step 315i of E0.12-T05-S02, the card's additions to the measures: (1) whether agentspace-tools.ps1's winget call worked when the patch run
# started it as SYSTEM; (2) whether the idle task's trigger (built with -RepetitionInterval and no -RepetitionDuration) repeats for as long as the
# host runs; (3) how many `Tools:` lines patch.log holds per patch run (also in 315a); and (4) the host's evidence for install.ps1's steps 1 to
# 8 and 10, localswim.ps1's steps 1 to 5 and bootstrap.ps1's own steps. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
#
# (1) is measured by one `winget --version` run as SYSTEM through a transient scheduled task of the host script, removed at its end, the way
# agentspace-tools.ps1 starts winget (`Invoke-AgentspaceProgram 'winget' @('--version')`); S02_SYSTEM_WINGET=0 skips it. The patch.log's
# `Tools: winget upgrade ... exited` lines are printed with it: the log says nothing when winget answers and upgrades cleanly.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315i' 'the card''s additions: winget as SYSTEM, the idle trigger, Tools: lines, and the evidence of each install, localswim and bootstrap step')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $withWinget = ($env:S02_SYSTEM_WINGET -ne '0')
    $h = Invoke-S02HostScript -Alias $alias -Name '315i' -Quiet -ScriptArgs $(if ($withWinget) { 'system-winget' } else { '' }) -TimeoutSeconds 300
    if ($null -ne $h) {
      $q = $h.Probe
      Write-S02Line '--- Evidence of each step, one line each (True means the host shows it) ---'
      $expect = @('install-1', 'install-2', 'install-3', 'install-4', 'install-5', 'install-6', 'install-7', 'install-8', 'install-10',
        'localswim-1', 'localswim-2', 'localswim-3', 'localswim-4', 'localswim-5',
        'bootstrap-PowerShell 7', 'bootstrap-Git for Windows', 'bootstrap-Node', 'bootstrap-uv', 'bootstrap-Claude Code', 'bootstrap-gh', 'bootstrap-login-shell', 'bootstrap-update-policy')
      $got = @{}
      if ($q.By.ContainsKey('evid')) { foreach ($v in $q.By['evid']) { $f = $v -split '\|', 3; if ($f.Count -eq 3) { $got[$f[0]] = $f; Write-S02Line "$($f[0]): $($f[1]); $($f[2])" } } }
      foreach ($id in $expect) {
        if (-not $got.ContainsKey($id)) { Add-S02Finding "The host printed no evidence line for $id."; continue }
        Assert-S02 ($got[$id][1] -ceq 'True') "$id is shown." "$id is NOT shown: $($got[$id][2])"
      }
      if ($withWinget) {
        $sx = Get-S02First $q 'system_winget_exit'
        Write-S02Line "winget as SYSTEM: who '$(Get-S02First $q 'system_winget_who')'; exit '$sx'; first line '$(Get-S02First $q 'system_winget_out')'; the transient task removed: $(Get-S02First $q 'system_winget_task_removed')"
        Assert-S02 ($null -ne $sx -and $sx -notin @('no-answer')) 'The SYSTEM winget probe answered.' 'The SYSTEM winget probe gave no answer within 90 s.'
        Assert-S02 ((Get-S02First $q 'system_winget_task_removed') -ceq 'True') 'The transient task was removed.' 'The transient task was not removed.'
        Write-S02Line ("RESULT (i, winget as SYSTEM): winget answers as SYSTEM: " + ($sx -ceq '0'))
      }
    }
    $f1 = Invoke-S02HostScript -Alias $alias -Name '315f' -Quiet
    if ($null -ne $f1) {
      $q = $f1.Probe
      $now = ConvertFrom-S02Utc (Get-S02First $q 'now_utc')
      $boot = ConvertFrom-S02Utc (Get-S02First $q 'boot_utc')
      $last = ConvertFrom-S02Utc (Get-S02First $q 'idle_task_last_run_utc')
      $next = ConvertFrom-S02Utc (Get-S02First $q 'idle_task_next_run_utc')
      $uptime = [int](Get-S02First $q 'uptime_seconds')
      Write-S02Line ("idle trigger: class $(Get-S02First $q 'idle_trigger_class'); delay '$(Get-S02First $q 'idle_trigger_delay')'; repetition interval '$(Get-S02First $q 'idle_trigger_interval')'; duration '$(Get-S02First $q 'idle_trigger_duration')'; stop at duration end '$(Get-S02First $q 'idle_trigger_stop_at_end')'")
      Write-S02Line ("idle task: last run $(Format-S02Time (Get-S02First $q 'idle_task_last_run_utc')); next run $(Format-S02Time (Get-S02First $q 'idle_task_next_run_utc')); last result $(Get-S02First $q 'idle_task_last_result'); missed runs $(Get-S02First $q 'idle_task_missed_runs'); host uptime $uptime s; lines in idle.log since boot $(Get-S02First $q 'idle_lines_since_boot')")
      Assert-S02 ((Get-S02First $q 'idle_trigger_interval') -ceq 'PT5M') 'The trigger repeats every 5 minutes.' "The trigger's repetition interval is '$(Get-S02First $q 'idle_trigger_interval')', expected PT5M."
      $dur = Get-S02First $q 'idle_trigger_duration'
      Assert-S02 ($dur -ceq '' -or $dur -ceq 'PT0S' -or $null -eq $dur) 'The repetition has no duration (it repeats without end).' "The repetition's duration is '$dur'."
      $ranThisBoot = ($null -ne $last -and $null -ne $boot -and $last -ge $boot)
      Assert-S02 $ranThisBoot 'The idle task has run since this boot.' 'The idle task has NOT run since this boot (its last run is before the boot): an AtStartup trigger registered after the boot does not fire until the next boot.'
      if ($ranThisBoot -and $null -ne $now) {
        $ago = [int][Math]::Round(($now - $last).TotalSeconds)
        Write-S02Line "The last run was $ago s before the read; uptime is $uptime s."
        Assert-S02 ($ago -le 400) 'The last run is within 400 s of now (a 5-minute repetition is running).' "The last run was $ago s ago, more than one 5-minute period: the repetition has stopped."
        if ($null -ne $next) { $ahead = [int][Math]::Round(($next - $now).TotalSeconds); Assert-S02 ($ahead -ge -30 -and $ahead -le 400) "The next run is $ahead s ahead." "The next run is $ahead s from now, not within one period." }
        else { Add-S02Finding 'The idle task has no next run time.' }
        $due = [Math]::Max(0, [int][Math]::Floor(($now - $boot).TotalSeconds / 300) - 2)
        $lines = [int](Get-S02First $q 'idle_lines_since_boot')
        Assert-S02 ($lines -ge $due) "idle.log holds $lines line(s) since boot, at least the $due that the uptime asks for." "idle.log holds $lines line(s) since boot, fewer than the $due that $uptime s of uptime asks for."
      }
    }
    $a = Invoke-S02HostScript -Alias $alias -Name '315a' -Quiet
    if ($null -ne $a) {
      $n = 0; $bad = 0
      if ($a.Probe.By.ContainsKey('run')) {
        foreach ($v in $a.Probe.By['run']) {
          $f = $v -split '\|', 8
          if ($f.Count -lt 8) { continue }
          $n++
          Write-S02Line "patch run $($f[0]): $($f[5]); $($f[6]); result $($f[4])"
          if ($f[4] -ceq 'finished' -and $f[6] -cne 'summary=1') { $bad++ }
        }
      }
      Write-S02Line "Tools: lines per patch run, from patch.log: $n run(s) read"
      Assert-S02 ($n -ge 1 -and $bad -eq 0) 'Every finished patch run holds exactly one Tools: version-summary line.' "$bad finished patch run(s) do not hold exactly one Tools: version-summary line, or no run was read."
    }
  })

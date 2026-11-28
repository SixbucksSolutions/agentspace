# Step 315b of E0.12-T05-S02, measure (b): whether a restart of the sshd service ends the sessions open at that moment. One session holds
# `sleep 120` while a second restarts sshd (`pwsh -Command Restart-Service sshd`); the first's exit code and end time are recorded, and sshd's
# process ID before and after shows that the restart happened. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# - It MUST run BEFORE Terry's sign-in session and never during it: it refuses (STOP, exit 5) while the state file signin-asked.flag exists,
#   which the runner writes at the moment it asks Terry to sign in.
# - Either answer of the measure is a PASS (exit 0) with a RESULT line; a FINDING is a failure to measure: the first session was gone before
#   the restart, sshd's process ID did not change, or sshd was not Running again within about 80 seconds.
# - The knobs S02_SLEEP_SECONDS (120) and S02_HOLD_SECONDS (10) are for the dry runs.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '315b' 'measure (b): does a restart of sshd end the sessions open at that moment')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
if (Test-S02State 'signin-asked.flag') { Stop-S02Step 'Terry has been asked to sign in (signin-asked.flag exists); sshd MUST NOT be restarted during or after his sign-in session.'; exit 5 }
exit (Invoke-S02Main {
    $alias = Read-S02State 'lease-w.alias'
    $sleep = Get-S02Knob 'S02_SLEEP_SECONDS' 120
    $hold = Get-S02Knob 'S02_HOLD_SECONDS' 10

    function Read-SshdFacts([switch]$Tolerant) {
      $h = Invoke-S02HostScript -Alias $alias -Name 'pid' -Quiet -Tolerant:$Tolerant
      if ($null -eq $h) { return $null }
      return [pscustomobject]@{ State = (Get-S02First $h.Probe 'sshd_state'); Pid = (Get-S02First $h.Probe 'sshd_pid'); Start = (Get-S02First $h.Probe 'sshd_start_utc') }
    }

    $before = Read-SshdFacts
    if ($null -eq $before) { return }
    Write-S02Line "sshd before: state $($before.State); process ID $($before.Pid); started $(Format-S02Time $before.Start)"
    Assert-S02 ($before.State -ceq 'Running' -and $before.Pid -match '^\d+$') 'sshd is Running before the restart.' "sshd is '$($before.State)' before the restart."

    $s1 = Start-S02Ssh -Alias $alias -Command "sleep $sleep # s02:sleep"
    $t1 = $s1.Started
    Write-S02Line ("Session 1 (sleep $sleep) started at " + (Get-S02LocalText $t1))
    $limit = [DateTime]::UtcNow.AddSeconds($hold)
    while ([DateTime]::UtcNow -lt $limit -and $null -ne $s1.Process -and -not $s1.Process.HasExited) { Start-Sleep -Milliseconds 250 }
    $gone = ($null -eq $s1.Process -or $s1.Process.HasExited)
    Assert-S02 (-not $gone) "Session 1 was still open $hold s after it started, when the restart began." "Session 1 was gone before the restart began (it ended within $hold s); the measure is not made."
    if ($gone) { [void](Wait-S02Tool -Handle $s1 -TimeoutSeconds 10); return }

    $t2 = [DateTime]::UtcNow
    $s2 = Invoke-S02Ssh -Alias $alias -Command "pwsh -NoProfile -NonInteractive -Command 'Restart-Service sshd' # s02:restart" -TimeoutSeconds 180
    Write-S02Line ("Session 2 (Restart-Service sshd) began " + (Get-S02LocalText $t2) + ", ended " + (Get-S02LocalText $s2.EndedUtc) + "; exit $($s2.ExitCode); $([int]$s2.ElapsedSeconds) s (255 is possible when the restart ends its own session)")
    Assert-S02 (-not $s2.TimedOut) 'Session 2 did not time out.' 'Session 2 timed out.'

    $r1 = Wait-S02Tool -Handle $s1 -TimeoutSeconds ($sleep + 90)
    $elapsed = [int][Math]::Round(($r1.EndedUtc - $t1).TotalSeconds)
    Write-S02Line ("Session 1 ended " + (Get-S02LocalText $r1.EndedUtc) + ", exit $($r1.ExitCode), after $elapsed s of a $sleep s sleep; restart began $([int][Math]::Round(($t2 - $t1).TotalSeconds)) s into it.")
    Assert-S02 (-not $r1.TimedOut) 'Session 1 ended by itself or by the restart.' 'Session 1 was still open long after its sleep; it was stopped.'
    $survived = (-not $r1.TimedOut -and $r1.ExitCode -eq 0 -and $elapsed -ge ($sleep - 5))
    if ($survived) { Write-S02Line "RESULT (b): a restart of sshd did NOT end the open session: it finished its $sleep s sleep with exit 0 at $elapsed s." }
    else { Write-S02Line "RESULT (b): a restart of sshd ENDED the open session: exit $($r1.ExitCode) at $elapsed s of a $sleep s sleep." }

    $after = $null
    for ($i = 0; $i -lt 8; $i++) {
      $f = Read-SshdFacts -Tolerant
      if ($null -ne $f) { $after = $f; if ($f.State -ceq 'Running') { break } }
      Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min(10, $hold)))
    }
    if ($null -eq $after) { Add-S02Finding 'sshd could not be read after the restart.'; return }
    Write-S02Line "sshd after: state $($after.State); process ID $($after.Pid); started $(Format-S02Time $after.Start)"
    Assert-S02 ($after.State -ceq 'Running') 'sshd is Running again.' "sshd is '$($after.State)' after the restart."
    Assert-S02 ($after.Pid -ne $before.Pid) 'The process ID changed: the restart happened.' 'sshd''s process ID did not change: no restart happened.'
  })

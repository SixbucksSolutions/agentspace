# Step 341 of E0.12-T05-S02: the guard's stop. A bounded loop (at most 35 minutes from step 340's end, past D56's 25) reading, read-only, the
# observer's edges.log at C:\Projects\agentspace\target\t05-lead\observe\edges.log for `agentspace.windows-amd64 ... -> stopped`; guard.log's new
# lines since the count step 340 took (their time, step and keyword classes only, never the line); then one read-only `aws ec2 describe-instances`
# filtered by the stack name agentspace-windows-amd64-host-agentspace, returning only State.Name and StateReason.Code. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing.
#
# Ends: 0 the host is stopped by the guard within D56's 25 minutes; 1 a finding (past 25 minutes, a failed or sign-in line in guard.log, no guard
# `stopped` line, a StateReason.Code that is not the API stop's); 3 the 35-minute bound with no stop edge; 6 the hours reached 4.5.
# The knobs S02_POLL_SECONDS (30) and S02_STOP_SECONDS (2100) are for the dry runs. The observer's reads come from the dev lead's own loop; this step
# only reads its files. D56: a host is `stopped` within the grace (15 minutes) plus 10 minutes of its last live lease's end.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '341' 'the guard stops the host within D56''s bound: edges.log, guard.log, describe-instances')) { exit 5 }
if (-not (Test-S02State 'release-end.txt')) { Stop-S02Step 'no release time recorded; step 340 has not released lease W.'; exit 5 }
if (-not (Test-S02State 'guard-baseline.txt')) { Stop-S02Step 'no guard.log baseline recorded; step 340 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $relEnd = ConvertFrom-S02Utc (Read-S02State 'release-end.txt')
    $baseText = Read-S02State 'guard-baseline.txt'
    $script:GuardSeen = if ($baseText -match '^\d+$') { [int]$baseText } else { 0 }
    if ($null -eq $relEnd) { Add-S02Finding 'The release time in the state file cannot be read.'; return }
    $poll = Get-S02Knob 'S02_POLL_SECONDS' 30
    $stopSeconds = Get-S02Knob 'S02_STOP_SECONDS' 2100
    $deadline = $relEnd.AddSeconds($stopSeconds)
    $edgesFile = Join-Path (Get-S02ObserveDir) 'edges.log'
    Write-S02Line ("Release ended " + (Format-S02Time (Read-S02State 'release-end.txt')) + "; the loop's bound is $(Get-S02LocalText $deadline); guard.log baseline $($script:GuardSeen) line(s).")
    $script:GuardClasses = @{ stopped = 0; unexplained = 0; failed = 0; signin = 0 }

    function Read-NewGuardLines {
      $lines = @(Read-S02LogLines (Get-S02GuardLogPath))
      for ($i = $script:GuardSeen; $i -lt $lines.Count; $i++) {
        $l = $lines[$i]
        $t = ($l -split ' ', 2)[0]
        $step = if ($l -match 'step=(\S+)') { $Matches[1] } else { '(none)' }
        $hostName = if ($l -match '^\S+ host=(\S+)') { $Matches[1] } else { '(none)' }
        $stopped = $l -cmatch ' stopped since='
        $unexplained = $l.Contains(' unexplained ')
        $failed = $l.Contains(' failed ')
        $signin = $l.Contains('sign-in has expired')
        if ($stopped) { $script:GuardClasses['stopped']++ }
        if ($unexplained) { $script:GuardClasses['unexplained']++ }
        if ($failed) { $script:GuardClasses['failed']++ }
        if ($signin) { $script:GuardClasses['signin']++ }
        Write-S02Line ("guard.log: $(Format-S02Time $t) host=$hostName step=$step stopped: $stopped; unexplained: $unexplained; failed: $failed; sign-in has expired: $signin")
      }
      if ($lines.Count -gt $script:GuardSeen) { $script:GuardSeen = $lines.Count }
    }

    $edge = $null
    while ($true) {
      if (Test-S02HoursReached) { Set-S02Exit 6; return }
      Read-NewGuardLines
      foreach ($l in (Read-S02LogLines $edgesFile)) {
        $m = [regex]::Match($l, '^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) \S+ agentspace\.windows-amd64 (\S+) -> stopped\b')
        if ($m.Success) { $at = ConvertFrom-S02Utc $m.Groups[1].Value; if ($null -ne $at -and $at -ge $relEnd) { $edge = $m.Groups[1].Value; break } }
      }
      if ($null -ne $edge) { break }
      if ([DateTime]::UtcNow -ge $deadline) {
        Add-S02Finding 'No `agentspace.windows-amd64 -> stopped` edge appeared in edges.log within 35 minutes of the release; D56''s bound is 25.'
        Set-S02Exit 3
        return
      }
      Write-S02Line ("Waiting at $(Get-S02NowText): no stop edge yet; hours $(Get-S02HoursText)")
      $left = [int][Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds)
      Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min($poll, $left)))
    }
    Start-Sleep -Seconds ([Math]::Min($poll, 5))
    Read-NewGuardLines
    $minutes = (($(ConvertFrom-S02Utc $edge) - $relEnd).TotalMinutes)
    Write-S02Line ("Stop edge seen by the observer: " + (Format-S02Time $edge) + "; $([Math]::Round($minutes, 1)) minute(s) after the release (the observer reads once a minute, so the stop was at most a minute earlier).")
    Assert-S02 ($minutes -le 25) 'The host was stopped within D56''s 25 minutes of the release.' "The host was seen stopped $([Math]::Round($minutes, 1)) minutes after the release, past D56's 25."
    Write-S02Line "guard.log new line classes: stopped $($script:GuardClasses['stopped']); unexplained $($script:GuardClasses['unexplained']); failed $($script:GuardClasses['failed']); sign-in has expired $($script:GuardClasses['signin'])"
    Assert-S02 ($script:GuardClasses['stopped'] -ge 1) 'guard.log shows a stopped line.' 'guard.log shows no `stopped` line since the release.'
    Assert-S02 ($script:GuardClasses['failed'] -eq 0 -and $script:GuardClasses['signin'] -eq 0) 'guard.log shows no failed or sign-in line.' 'guard.log shows a failed or sign-in line since the release.'

    $query = 'Reservations[].Instances[].[State.Name,StateReason.Code]'
    $r = Invoke-S02Tool -Tool 'aws' -Arguments @('ec2', 'describe-instances', '--filters', 'Name=tag:aws:cloudformation:stack-name,Values=agentspace-windows-amd64-host-agentspace', '--query', $query, '--output', 'text') -TimeoutSeconds 120
    Write-S02Line "describe-instances exit: $($r.ExitCode)"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'describe-instances exited 0.' "describe-instances exited $($r.ExitCode)."
    $rows = @($r.Stdout -split "\r?\n" | Where-Object { $_.Trim() -ne '' })
    Assert-S02 ($rows.Count -eq 1) 'The stack has one instance.' "describe-instances returned $($rows.Count) row(s), expected 1."
    foreach ($row in $rows) {
      $f = $row -split "\s+"
      $name = $f[0]; $code = if ($f.Count -ge 2) { $f[1] } else { '' }
      Write-S02Line "instance state: $name; StateReason.Code: $code"
      Assert-S02 ($name -ceq 'stopped') 'The instance is stopped.' "The instance is '$name', not stopped."
      Assert-S02 ($code -ceq 'Client.UserInitiatedShutdown') 'StateReason.Code is Client.UserInitiatedShutdown: an API stop, the guard''s.' "StateReason.Code is '$code', not the API stop's Client.UserInitiatedShutdown (Client.InstanceInitiatedShutdown is the host's own power-off, the idle check's)."
    }
  })

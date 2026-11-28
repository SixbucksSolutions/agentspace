# Step 311 of E0.12-T05-S02: the wait. `lease get` once a minute, bounded at 90 minutes, each read's time and state, and the
# masked new lines of the host's provision.log since step 310's baseline, with each phase's first line time kept for the
# record. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# Ends: 0 the lease is ready; 1 `lease get` failed or printed no state (a failure is a finding, never retried past); 2 the lease is
# unsatisfiable, expired or released, or a provision.log line says a step failed; 3 the 90-minute bound; 6 windows/amd64's hours
# reached 4.5.
# The provision.log of a detached run holds step lines only (operator.md, provision, "Its log"): `run begun`, then `deploy`,
# `start` (the first connection), `sync` (with its patch wait), `bootstrap`, `record` and `place`, each `done` or `failed`,
# then `run done`. The knobs S02_POLL_SECONDS (60) and S02_MAX_SECONDS (5400) are for the dry runs.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '311' 'the wait: lease get once a minute, bounded at 90 minutes, with the provisioning log')) { exit 5 }
if (-not (Test-S02State 'lease-w.id')) { Stop-S02Step 'no lease W state file; step 310 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $id = Read-S02State 'lease-w.id'
    $poll = Get-S02Knob 'S02_POLL_SECONDS' 60
    $max = Get-S02Knob 'S02_MAX_SECONDS' 5400
    $baseText = Read-S02State 'provlog-baseline.txt'
    $script:Seen = if ($baseText -match '^\d+$') { [int]$baseText } else { 0 }
    $logPath = Get-S02ProvisionLogPath
    $script:Phases = [ordered]@{}
    $started = [DateTime]::UtcNow
    $deadline = $started.AddSeconds($max)
    $n = 0
    $state = ''
    $script:FailedStep = $false
    Write-S02Line "Poll every $poll s; bound $max s; provision.log baseline $script:Seen line(s)."

    function Read-NewLogLines {
      $lines = @(Read-S02LogLines $logPath)
      if ($lines.Count -lt $script:Seen) { Add-S02Finding "provision.log has $($lines.Count) line(s), fewer than the baseline $script:Seen."; $script:seen = $lines.Count; return }
      for ($i = $script:Seen; $i -lt $lines.Count; $i++) {
        $l = $lines[$i]
        Write-S02Line ("log: " + (Format-S02LogLine $l))
        $m = [regex]::Match($l, '^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (run|deploy|start|sync|bootstrap|record|place|success) (begun|done|failed|refused|left)')
        if ($m.Success) {
          $key = $m.Groups[2].Value + ' ' + $m.Groups[3].Value
          if (-not $script:Phases.Contains($key)) { $script:Phases[$key] = $m.Groups[1].Value }
          if ($m.Groups[3].Value -ceq 'failed' -and -not $script:failedStep) { $script:failedStep = $true; Add-S02Finding ("provision.log says '" + $key + "' (the line above); a failure is a finding, not retried past.") }
        }
      }
      $script:seen = $lines.Count
    }

    while ($true) {
      $n++
      if (Test-S02HoursReached) { Set-S02Exit 6; return }
      $r = Invoke-S02Tool -Tool 'agentspace' -Arguments @('lease', 'get', $id) -TimeoutSeconds 120
      $doc = ConvertFrom-S02Json $r.Stdout.TrimEnd("`n")
      $state = if ($null -ne $doc -and $doc -is [System.Collections.IDictionary]) { [string]$doc['state'] } else { '' }
      Write-S02Line ("Read $n at $(Get-S02NowText): lease get exit $($r.ExitCode), state '$state'; hours so far $(Get-S02HoursText)")
      if ($r.ExitCode -ne 0 -or $state -eq '') { Add-S02Finding "lease get exit $($r.ExitCode) with no readable state; a failure is a finding."; return }
      Read-NewLogLines
      if ($script:FailedStep) { Set-S02Exit 2; return }
      if ($state -ceq 'ready') { break }
      if ($state -cin @('unsatisfiable', 'expired', 'released')) {
        Add-S02Finding "The lease is '$state', not ready."
        Set-S02Exit 2
        return
      }
      if ([DateTime]::UtcNow -ge $deadline) {
        Add-S02Finding "The lease is still '$state' after $max s; the bound is reached."
        Set-S02Exit 3
        return
      }
      $left = [int][Math]::Ceiling(($deadline - [DateTime]::UtcNow).TotalSeconds)
      Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min($poll, $left)))
    }

    Write-S02Line "Ready at read $n, $(Get-S02NowText)."
    # `run done` comes just after the lease is placed; give it up to six short looks.
    for ($k = 0; $k -lt 6 -and -not $script:Phases.Contains('run done'); $k++) {
      Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min($poll, 10)))
      Read-NewLogLines
    }
    Write-S02Line 'Phase times, the first line of each (local time, then the log''s UTC):'
    foreach ($k in $script:Phases.Keys) { Write-S02Line ('  ' + $k + ': ' + (Format-S02Time $script:Phases[$k])) }
    $order = @('run begun', 'deploy done', 'start done', 'sync done', 'bootstrap done', 'record done', 'place done', 'run done')
    $prev = $null
    foreach ($k in $order) {
      if (-not $script:Phases.Contains($k)) { Write-S02Line "  $k`: not seen"; continue }
      $t = ConvertFrom-S02Utc $script:Phases[$k]
      if ($null -ne $prev -and $null -ne $t) { Write-S02Line ("  " + $k + ' took ' + ([int][Math]::Round(($t - $prev).TotalSeconds)) + ' s after the line before') }
      $prev = $t
    }
    Assert-S02 ($script:Phases.Contains('run begun')) 'The run began.' 'provision.log shows no "run begun" line since the baseline.'
    foreach ($k in 'deploy done', 'start done', 'sync done', 'bootstrap done', 'record done', 'place done') {
      Assert-S02 ($script:Phases.Contains($k)) "The log shows '$k'." "The log shows no '$k' line since the baseline."
    }
    if (-not $script:Phases.Contains('run done')) { Write-S02Line 'NOTE: no "run done" line yet; read the log again later.' }
    Write-S02Line "Hours so far: $(Get-S02HoursText)"
  })

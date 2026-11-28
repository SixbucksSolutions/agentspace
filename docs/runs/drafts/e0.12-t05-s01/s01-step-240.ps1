# Step 240: the guard's stop. A bounded loop, at most 35 minutes from 230's end, reads the observer's edges.log (read only)
# for `agentspace.linux-amd64 ... -> stopped`; then guard.log's new lines since 230's count (time, step and keyword classes
# only), and the stop's attribution by one read-only aws ec2 describe-instances. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing.
# Exit 0 PASS; 1 a check failed; 3 no stop was seen by the bound; 6 the observer's hours (read at every pass).
# D56's bound is 25 minutes from the last lease's end; the observer reads once a minute, so a stop SEEN up to 26 minutes
# after the end is a NOTE that passes, and one later is a finding (a question for the dev lead).
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')

function Get-S01Classes([string]$Line) {
  $ts = ($Line -split '\s+')[0]
  $step = if ($Line -cmatch 'step=(\S+)') { $Matches[1] } else { '(none)' }
  $ours = $Line.Contains('host=' + $script:S01HostName + ' ')
  $stopped = $Line -cmatch '\bstopped\b'
  $unexpl = $Line -cmatch '\bunexplained\b'
  $failed = $Line -cmatch '\bfailed\b'
  $signin = $Line.Contains('sign-in has expired')
  return @{ Time = $ts; Step = $step; Ours = $ours; Stopped = $stopped; Unexplained = $unexpl; Failed = $failed; SignIn = $signin }
}

exit (Invoke-S01Main {
    $stop = Start-S01Step '240' 'the guard stops the host after the release'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $endText = Get-S01State 'end-230.utc'
    $end = ConvertTo-S01Time $endText
    $baseText = Get-S01State 'guard-230.count'
    if ($null -eq $end -or $baseText -cnotmatch '^\d+$') { Write-S01 'FINDING: The state file holds no end time or no guard.log count from step 230; it has not run.'; return 1 }
    $baseline = [int]$baseText
    $poll = if ($env:S01_POLL_SECONDS) { [int]$env:S01_POLL_SECONDS } else { 30 }
    $boundSec = if ($env:S01_BOUND_SECONDS) { [int]$env:S01_BOUND_SECONDS } else { 2100 }
    $deadline = $end.AddSeconds($boundSec)
    Write-S01 "Step 230 ended at $(ConvertTo-S01Local $end); the guard is due by $(ConvertTo-S01Local $end.AddMinutes(25)); this loop reads to $(ConvertTo-S01Local $deadline) at most, every $poll seconds."
    $edgesPath = Join-Path (Get-S01ObserveDir) 'edges.log'
    $found = $null
    $pass = 0
    while ($true) {
      $pass++
      $h = Test-S01Hours "Pass $pass"
      if ($null -ne $h) { return $h }
      $edges = Read-S01LogLines $edgesPath
      if ($null -eq $edges) { Write-S01 "Pass ${pass} at $(Get-S01Clock): edges.log is not readable."; $edges = @() }
      foreach ($e in $edges) {
        if ($e -cmatch '^(\S+) (\S+) agentspace\.linux-amd64 (\S+) -> stopped\b') {
          $t = ConvertTo-S01Time $Matches[1]
          if ($null -ne $t -and $t -ge $end) { $found = @{ Line = $e; Time = $t }; break }
        }
      }
      Write-S01 "Pass ${pass} at $(Get-S01Clock): edges.log has $($edges.Count) line(s); a stopped edge of $($script:S01HostName) after step 230's end: $($null -ne $found)"
      if ($null -ne $found) { break }
      if ([DateTimeOffset]::UtcNow.AddSeconds($poll) -gt $deadline) {
        Write-S01 "FINDING: no stopped edge of $($script:S01HostName) was seen by the bound ($(ConvertTo-S01Local $deadline)); the host may still run. The runner stops and reports."
        return 3
      }
      Start-Sleep -Seconds $poll
    }
    Write-S01 ('Edge: ' + $found.Line)
    $mins = ($found.Time - $end).TotalMinutes
    Write-S01 ('The stop was seen {0:N1} minutes after step 230 ended (D56: grace 15 plus 10 = 25; the observer reads once a minute).' -f $mins)
    if ($mins -le 25) { Write-S01 'Within D56''s bound of 25 minutes: True' }
    elseif ($mins -le 26) { Write-S01 'NOTE: seen 25 to 26 minutes after the end, within one read of the observer''s spacing; this passes.' }
    else { $p.Add(("the stop was seen {0:N1} minutes after the last lease's end, past D56's 25 minutes and one read of the observer" -f $mins)) }

    # guard.log's new lines, as time, step and classes only.
    $glog = Read-S01LogLines (Get-S01GuardLogPath)
    if ($null -eq $glog) { $glog = @() }
    $new = @(if ($glog.Count -gt $baseline) { $glog[$baseline..($glog.Count - 1)] })
    Write-S01 "guard.log lines now: $($glog.Count) (baseline $baseline, new $($new.Count))"
    $oursStopped = 0
    foreach ($l in $new) {
      $c = Get-S01Classes $l
      $lt = ConvertTo-S01Time $c.Time
      $when = if ($null -ne $lt) { "$(ConvertTo-S01Local $lt) ($($c.Time))" } else { $c.Time }
      Write-S01 "  $when step=$($c.Step) host $($script:S01HostName): $($c.Ours); stopped: $($c.Stopped); unexplained: $($c.Unexplained); failed: $($c.Failed); sign-in has expired: $($c.SignIn)"
      if ($c.Ours -and $c.Stopped -and -not $c.Failed) { $oursStopped++ }
      if ($c.Failed) { Write-S01 '  NOTE: a new guard.log line is a failure; the stop may still have been made by another run.' }
    }
    Write-S01 "New guard.log lines that say $($script:S01HostName) stopped: $oursStopped"
    if ($oursStopped -lt 1) { $p.Add("guard.log holds no new line for host=$($script:S01HostName) stopped, so the guard did not log the stop") }

    # The attribution, one read-only call.
    # The region is read at run time from where the operator's runner takes it, agentspace-run's REGION constant,
    # so this file holds no region (architect, 2026-11-27, for v0.12-T05). It is never printed.
    $region = $null
    $dir = $PSScriptRoot
    while ($dir -and -not $region) {
      $src = Join-Path (Join-Path (Join-Path (Join-Path $dir 'crates') 'agentspace-run') 'src') 'lib.rs'
      if (Test-Path -LiteralPath $src) {
        $line = Select-String -LiteralPath $src -Pattern '^const REGION: &str = "([^"]+)";' | Select-Object -First 1
        if ($line) { $region = $line.Matches[0].Groups[1].Value }
      }
      $dir = Split-Path -Parent $dir
    }
    if (-not $region) { $p.Add('the operator region could not be read from agentspace-run''s lib.rs') }
    $rows = @()
    $form = ''
    foreach ($filter in @(if ($region) { "Name=tag:aws:cloudformation:stack-name,Values=$($script:S01Stack)", "Name=tag:project,Values=$($script:S01Stack)" })) {
      Write-S01 "Command: aws ec2 describe-instances, filtered by a stack tag on the stack $($script:S01Stack), asking State.Name and StateReason.Code only"
      $run = Invoke-S01Tool -Tool 'aws' -Arguments @('ec2', 'describe-instances', '--region', $region, '--filters', $filter, '--query', 'Reservations[].Instances[].[State.Name,StateReason.Code]', '--output', 'text') -TimeoutSec 90
      Write-S01 "Exit: $($run.ExitCode)"
      if ($run.StartFailed -or $run.TimedOut -or $run.ExitCode -ne 0) {
        $p.Add("aws ec2 describe-instances exited $($run.ExitCode), expected 0")
        if ($run.StderrBytes.Length -gt 0) { Write-S01 ('aws error (masked): ' + (Get-S01Excerpt $run)) }
        $rows = $null
        break
      }
      $rows = @($run.Stdout -split "\r?\n" | Where-Object { $_.Trim() -ne '' } | ForEach-Object { $c = @($_.Trim() -split '\s+'); [pscustomobject]@{ State = $c[0]; Code = $(if ($c.Count -gt 1) { $c[1] } else { '(none)' }) } })
      $form = $filter.Split(',')[0]
      if ($rows.Count -gt 0) { break }
    }
    if ($null -ne $rows) {
      Write-S01 "Instances read: $($rows.Count); filter used: $form"
      $live = @($rows | Where-Object { $_.State -cne 'terminated' })
      foreach ($r in $rows) { Write-S01 "  State.Name '$($r.State)', StateReason.Code '$($r.Code)'" }
      if ($live.Count -ne 1) { $p.Add("$($live.Count) instance(s) of the stack are not terminated, expected 1") }
      else {
        $code = $live[0].Code
        if ($live[0].State -cne 'stopped') { $p.Add("the instance is '$($live[0].State)', expected 'stopped'") }
        if ($code -ceq 'Client.UserInitiatedShutdown') { Write-S01 'Attribution: an API stop, as the guard makes it (Client.UserInitiatedShutdown).' }
        elseif ($code -ceq 'Client.InstanceInitiatedShutdown') { $p.Add('the stop is Client.InstanceInitiatedShutdown, the host''s own power-off (the idle check), not the guard''s API stop') }
        else { $p.Add("the stop's StateReason.Code is '$code', expected Client.UserInitiatedShutdown") }
      }
    }
    return (Complete-S01 $p)
  })

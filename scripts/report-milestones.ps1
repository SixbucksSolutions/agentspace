#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Reports agentspace's milestones to Terry's swimcoach, following
# C:\Projects\swimcoach\docs\agents\swimcoach-reporting.md. Each milestone in milestones.json is a
# set of board cards; its lanes are counted from the agentspace localswim board.
#
#   pwsh -NoProfile -File scripts\report-milestones.ps1          # report once, then exit
#   pwsh -NoProfile -File scripts\report-milestones.ps1 -Watch   # keep reporting
#   pwsh -NoProfile -File scripts\report-milestones.ps1 -DryRun  # print what would be sent
#
# With -Watch it polls the board every minute and reports a milestone when its lanes changed and 5
# minutes have passed since its last report, or when 10 minutes have passed regardless. It exits on
# a swimcoach authentication or config failure (exit 5 or 6), which is Terry's to fix.
param([switch]$Watch, [switch]$DryRun)

$ErrorActionPreference = 'Stop'
$Board = 'c74ddc68-9c3a-4434-8435-563c596d3229'
$Localswim = 'C:\Projects\localswim\localswim.exe'
$Swimcoach = 'C:\Projects\swimcoach\bin\swimcoach.cmd'
$Config = Join-Path $PSScriptRoot 'milestones.json'

function Get-Cards {
  . C:\Projects\claude-localswim-watcher\lib\env.ps1
  Set-LocalswimEnv
  try { $b = & $Localswim board get --view lanes $Board | ConvertFrom-Json -Depth 20 }
  finally { Clear-LocalswimEnv }
  $keys = @{}
  foreach ($l in $b.lanes) { $keys[$l.id] = $l.key }
  foreach ($c in $b.cards) { [pscustomobject]@{ Ticket = [int]$c.ticket; Lane = $keys[$c.lane_id]; Title = $c.title } }
}

function New-Report($m, $cards, [double]$active) {
  $mine = @($cards | Where-Object { $m.tickets -contains $_.Ticket })
  $count = { param($k) @($mine | Where-Object Lane -eq $k).Count }
  $report = [ordered]@{
    agent        = 'claude'
    project      = 'agentspace'
    milestone    = $m.name
    active_hours = [Math]::Min([Math]::Max([Math]::Round($active, 2), 0), 168)
    cap_hours    = $m.cap_hours
    lanes        = [ordered]@{
      ready_for_work   = & $count 'ready_for_work'
      in_progress      = & $count 'in_progress'
      needs_terry      = & $count 'needs_terry_action'
      ready_for_review = & $count 'ready_for_review'
      completed        = & $count 'completed'
    }
  }
  $waiting = @($mine | Where-Object Lane -eq 'needs_terry_action' | Sort-Object Ticket)
  if ($waiting) {
    $label = ($waiting[0].Title -split ':')[0]
    $more = if ($waiting.Count -gt 1) { " and $($waiting.Count - 1) more" } else { '' }
    $report.needs_me = "$label$more on the agentspace board waits on you; see its card."
  }
  $report
}

function Send-Report($report) {
  $json = $report | ConvertTo-Json -Depth 5 -Compress
  $out = if ($DryRun) { $json | & $Swimcoach report --dry-run --file - 2>&1 | Out-String }
         else { $json | & $Swimcoach report --file - 2>&1 | Out-String }
  [pscustomobject]@{ Code = $LASTEXITCODE; Out = $out.Trim() }
}

# Per milestone, kept across runs under the profile, never AppData (D21):
# - when it was last reported, and its lanes then, so a restart never reports again inside
#   swimcoach's 5-minute limit;
# - its active hours, and when they were last counted. The clock runs only while at least one of
#   the milestone's cards is In Progress (Terry, 2026-11-17); a card waiting on Terry sits in
#   Needs Terry, which pauses it. Each poll adds at most 2 minutes, so time this script was not
#   running is never counted.
$StateFile = Join-Path $env:USERPROFILE '.agentspace\swimcoach-last.json'
$state = @{}
if (Test-Path $StateFile) { $state = Get-Content $StateFile -Raw | ConvertFrom-Json -AsHashtable }
function Save-State {
  New-Item -ItemType Directory -Force (Split-Path $StateFile) | Out-Null
  $state | ConvertTo-Json -Compress | Set-Content -NoNewline $StateFile
}
function Get-Time($v) { if ($v -is [DateTime]) { $v.ToUniversalTime() } else { ([DateTimeOffset]::Parse("$v")).UtcDateTime } }
while ($true) {
  $milestones = (Get-Content $Config -Raw | ConvertFrom-Json).milestones
  $cards = @(Get-Cards)
  $now = [DateTime]::UtcNow
  foreach ($m in $milestones) {
    $st = $state[$m.name]
    if (-not $st) { $st = @{}; $state[$m.name] = $st }
    if ($null -eq $st.active) {
      # First run for this milestone: start from the wall-clock time since it began.
      $st.active = [Math]::Max(([DateTime]::UtcNow - (Get-Time $m.start)).TotalHours, 0)
    }
    $inProgress = @($cards | Where-Object { $m.tickets -contains $_.Ticket -and $_.Lane -eq 'in_progress' }).Count
    if ($st.polled -and $inProgress -gt 0) {
      $dt = [Math]::Min(($now - (Get-Time $st.polled)).TotalMinutes, 2)
      if ($dt -gt 0) { $st.active = [double]$st.active + $dt / 60 }
    }
    if (-not $DryRun) { $st.polled = $now.ToString('o'); Save-State }

    $report = New-Report $m $cards ([double]$st.active)
    $lanes = $report.lanes | ConvertTo-Json -Compress
    $age = if ($st.at) { ($now - (Get-Time $st.at)).TotalMinutes } else { [double]::MaxValue }
    $changed = $st.lanes -ne $lanes
    if (-not ($age -ge 10 -or ($changed -and $age -ge 5))) { continue }
    $r = Send-Report $report
    $stamp = Get-Date -Format 'HH:mm:ss'
    switch ($r.Code) {
      0 {
        if (-not $DryRun) { $st.at = $now.ToString('o'); $st.lanes = $lanes; Save-State }
        Write-Host "$stamp $($m.name): $($r.Out)"
      }
      { $_ -in 5, 6 } { Write-Output "$stamp $($m.name): swimcoach exit $($r.Code), Terry's to fix: $($r.Out)"; exit $r.Code }
      default { if (-not $DryRun) { $st.at = $now.ToString('o'); Save-State }; Write-Output "$stamp $($m.name): swimcoach exit $($r.Code): $($r.Out)" }
    }
  }
  if (-not $Watch) { break }
  Start-Sleep -Seconds 60
}

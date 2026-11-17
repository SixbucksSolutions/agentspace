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

function New-Report($m, $cards) {
  $mine = @($cards | Where-Object { $m.tickets -contains $_.Ticket })
  $count = { param($k) @($mine | Where-Object Lane -eq $k).Count }
  # ConvertFrom-Json already turns an ISO time into a DateTime; take it as UTC either way.
  $start = if ($m.start -is [DateTime]) { $m.start.ToUniversalTime() } else { [DateTimeOffset]::Parse($m.start).UtcDateTime }
  $hours = [Math]::Round(([DateTime]::UtcNow - $start).TotalHours, 2)
  $report = [ordered]@{
    agent        = 'claude'
    project      = 'agentspace'
    milestone    = $m.name
    active_hours = [Math]::Min([Math]::Max($hours, 0), 168)
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

# When each milestone was last reported, kept across runs so a restart never reports again inside
# swimcoach's 5-minute limit. Under the profile, never AppData (D21).
$StateFile = Join-Path $env:USERPROFILE '.agentspace\swimcoach-last.json'
$last = @{}
$lastLanes = @{}
if (Test-Path $StateFile) {
  $saved = Get-Content $StateFile -Raw | ConvertFrom-Json -AsHashtable
  foreach ($k in $saved.Keys) {
    $last[$k] = ([DateTimeOffset]::Parse("$($saved[$k].at)")).UtcDateTime
    $lastLanes[$k] = $saved[$k].lanes
  }
}
function Save-State {
  $o = @{}
  foreach ($k in $last.Keys) { $o[$k] = @{ at = $last[$k].ToString('o'); lanes = $lastLanes[$k] } }
  New-Item -ItemType Directory -Force (Split-Path $StateFile) | Out-Null
  $o | ConvertTo-Json -Compress | Set-Content -NoNewline $StateFile
}
while ($true) {
  $milestones = (Get-Content $Config -Raw | ConvertFrom-Json).milestones
  $cards = @(Get-Cards)
  $now = [DateTime]::UtcNow
  foreach ($m in $milestones) {
    $report = New-Report $m $cards
    $lanes = $report.lanes | ConvertTo-Json -Compress
    $age = if ($last.ContainsKey($m.name)) { ($now - $last[$m.name]).TotalMinutes } else { [double]::MaxValue }
    $changed = $lastLanes[$m.name] -ne $lanes
    if (-not ($age -ge 10 -or ($changed -and $age -ge 5))) { continue }
    $r = Send-Report $report
    $stamp = Get-Date -Format 'HH:mm:ss'
    switch ($r.Code) {
      0 {
        if (-not $DryRun) { $last[$m.name] = $now; $lastLanes[$m.name] = $lanes; Save-State }
        Write-Host "$stamp $($m.name): $($r.Out)"
      }
      { $_ -in 5, 6 } { Write-Output "$stamp $($m.name): swimcoach exit $($r.Code), Terry's to fix: $($r.Out)"; exit $r.Code }
      default { if (-not $DryRun) { $last[$m.name] = $now; Save-State }; Write-Output "$stamp $($m.name): swimcoach exit $($r.Code): $($r.Out)" }
    }
  }
  if (-not $Watch) { break }
  Start-Sleep -Seconds 60
}

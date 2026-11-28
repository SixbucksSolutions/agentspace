param([switch]$Planted)
$state = Join-Path $env:USERPROFILE '.agentspace'
$lock = Join-Path $state 'board-holder-keeper.lock'
$facts = Join-Path (Get-Location).Path 'target\t05\installer-facts.txt'
$line = (Get-Content -LiteralPath $facts | Where-Object { $_ -like 'copy_end_utc=*' } | Select-Object -First 1)
$copyEnd = [DateTime]::Parse($line.Substring('copy_end_utc='.Length), [Globalization.CultureInfo]::InvariantCulture, 'RoundtripKind').ToUniversalTime()
$kpid = $null
for ($i = 0; $i -lt 20 -and $null -eq $kpid; $i++) {
  try {
    $fs = [IO.File]::Open($lock, 'Open', 'Read', 'ReadWrite')
    try { $t = (New-Object IO.StreamReader($fs)).ReadToEnd().Trim() } finally { $fs.Dispose() }
    if ($t -match '^\d+$' -and (Get-Process -Id ([int]$t) -ErrorAction SilentlyContinue)) { $kpid = [int]$t }
  } catch { }
  if ($null -eq $kpid) { Start-Sleep -Milliseconds 500 }
}
if ($null -eq $kpid) { 'FINDING: no keeper running'; exit 5 }
$p = Get-Process -Id $kpid
"keeper process name: $($p.ProcessName)"
$ks = $p.StartTime.ToUniversalTime()
if ($Planted) { $copyEnd = $ks.AddMinutes(1); 'planted copy end: one minute after the keeper start' }
$ci = [Globalization.CultureInfo]::InvariantCulture
"keeper start local: $($p.StartTime.ToString('h:mm:ss', $ci))$($p.StartTime.ToString('tt', $ci).ToLowerInvariant())"
"copy end local: $($copyEnd.ToLocalTime().ToString('h:mm:ss.fff', $ci))$($copyEnd.ToLocalTime().ToString('tt', $ci).ToLowerInvariant())"
$after = $ks -gt $copyEnd
"started after the copy's end: $after"
if ($p.ProcessName -ne 'agentspace-keeper') { 'FINDING: the process is not the keeper'; exit 5 }
if (-not $after) { 'FINDING: the keeper started before the copy ended, so it is the old build'; exit 5 }

$ErrorActionPreference = 'Continue'
$taskName = 'agentspace board holder'
$state = Join-Path $env:USERPROFILE '.agentspace'
$bin = Join-Path $state 'bin'
$lock = Join-Path $state 'board-holder-keeper.lock'
$facts = Join-Path (Get-Location).Path 'target\t05\installer-facts.txt'
$dist = Join-Path (Get-Location).Path 'target\dist'

function Get-KeeperPid {
  try {
    $fs = [IO.File]::Open($lock, 'Open', 'Read', 'ReadWrite')
    try { $sr = New-Object IO.StreamReader($fs); $t = $sr.ReadToEnd().Trim() } finally { $fs.Dispose() }
    if ($t -match '^\d+$') { return [int]$t }
  } catch { }
  return $null
}
function Stop-CheckedKeeper([int]$p) {
  $proc = Get-Process -Id $p -ErrorAction SilentlyContinue
  if (-not $proc) { return 'not running' }
  if ($proc.ProcessName -ne 'agentspace-keeper') { return 'NOT A KEEPER; left alone' }
  Stop-Process -Id $p -Force
  $null = Wait-Process -Id $p -Timeout 30 -ErrorAction SilentlyContinue
  if (Get-Process -Id $p -ErrorAction SilentlyContinue) { return 'STILL RUNNING' }
  return 'stopped'
}

Stop-ScheduledTask -TaskName $taskName
"task run ended: True"
$oldPid = Get-KeeperPid
"old keeper pid present: $($null -ne $oldPid)"
if ($null -ne $oldPid) { "old keeper: $(Stop-CheckedKeeper $oldPid)" }
Start-Sleep -Milliseconds 300

$files = @(Get-ChildItem -LiteralPath $dist -File -Name)
"dist files: $($files.Count)"
"dist files equal 8: $($files.Count -eq 8)"
if ($files.Count -ne 8) { 'REFUSED: not 8 files; nothing copied'; Start-ScheduledTask -TaskName $taskName; exit 6 }
New-Item -ItemType Directory -Force -Path $bin | Out-Null
$copied = 0; $aside = 0; $failed = 0
foreach ($f in $files) {
  $src = Join-Path $dist $f
  $dst = Join-Path $bin $f
  try { Copy-Item -LiteralPath $src -Destination $dst -Force -ErrorAction Stop; $copied++; continue } catch { }
  try {
    if (Test-Path -LiteralPath $dst) {
      $suffix = [guid]::NewGuid().ToString('N')
      Rename-Item -LiteralPath $dst -NewName ($f + '.old-' + $suffix) -ErrorAction Stop
      $aside++
    }
    Copy-Item -LiteralPath $src -Destination $dst -Force -ErrorAction Stop
    $copied++
  } catch { $failed++ }
}
$copyEnd = [DateTime]::UtcNow
"copied: $copied of $($files.Count); renamed aside: $aside; failed: $failed"
"copied equal 8: $($copied -eq 8)"
Set-Content -LiteralPath $facts -Encoding utf8 -Value ('copy_end_utc=' + $copyEnd.ToString('o'))

$mid = Get-KeeperPid
if ($null -ne $mid -and (Get-Process -Id $mid -ErrorAction SilentlyContinue)) {
  "keeper present at the copy's end: True; $(Stop-CheckedKeeper $mid)"
} else { "keeper present at the copy's end: False" }

$removed = 0; $inUse = 0
foreach ($o in @(Get-ChildItem -LiteralPath $bin -File -Name -Filter '*.old-*')) {
  try { Remove-Item -LiteralPath (Join-Path $bin $o) -Force -ErrorAction Stop; $removed++ } catch { $inUse++ }
}
".old- removed: $removed; in use: $inUse"

Start-ScheduledTask -TaskName $taskName
"task started: True"
if ($failed -gt 0) { exit 4 }

$d = Join-Path $env:USERPROFILE '.agentspace'
$facts = Join-Path (Get-Location).Path 'target\t05\installer-baselines.txt'
$b = @{}
foreach ($l in Get-Content -LiteralPath $facts) { $k, $v = $l -split '=', 2; $b[$k] = [int]$v }
function Read-LogLines([string]$p) {
  $fs = [IO.File]::Open($p, 'Open', 'Read', 'ReadWrite')
  try { $sr = New-Object IO.StreamReader($fs); $t = $sr.ReadToEnd() } finally { $fs.Dispose() }
  return @($t -split "`r?`n" | Where-Object { $_ -ne '' })
}
$bh = Read-LogLines (Join-Path $d 'board-holder.log')
$gl = Read-LogLines (Join-Path $d 'guard.log')
$bb = $b['board-holder.log']; $gb = $b['guard.log']
"board-holder.log lines now: $($bh.Count) (was $bb, new $($bh.Count - $bb))"
"guard.log lines now: $($gl.Count) (was $gb, new $($gl.Count - $gb))"
'--- board-holder.log new lines ---'
if ($bh.Count -gt $bb) { $bh[$bb..($bh.Count - 1)] | ForEach-Object { $_ -replace 'pid=\d+', 'pid=<pid>' } }
'--- guard.log new lines: time, step, classes ---'
if ($gl.Count -gt $gb) {
  foreach ($l in $gl[$gb..($gl.Count - 1)]) {
    $ts = ($l -split '\s+')[0]
    $step = if ($l -match 'step=(\S+)') { $Matches[1] } else { '(none)' }
    "$ts step=$step failed: $($l.Contains('failed')) sign-in has expired: $($l.Contains('sign-in has expired')) stopped: $($l.Contains('stopped')) unexplained: $($l.Contains('unexplained'))"
  }
}

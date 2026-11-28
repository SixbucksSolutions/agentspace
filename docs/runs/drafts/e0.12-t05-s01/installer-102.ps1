$d = Join-Path $env:USERPROFILE '.agentspace'
$facts = Join-Path (Get-Location).Path 'target\t05\installer-baselines.txt'
$out = @()
foreach ($n in 'board-holder.log', 'guard.log') {
  $p = Join-Path $d $n
  if (Test-Path -LiteralPath $p) { $c = @([IO.File]::ReadAllLines($p)).Count; "$n lines: $c"; $out += "$n=$c" } else { "$n missing"; $out += "$n=0" }
}
Set-Content -LiteralPath $facts -Encoding utf8 -Value $out

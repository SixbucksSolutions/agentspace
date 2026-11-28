# Shared by the stand-ins of E0.12-T05-S02's dry runs (dot-sourced; drafter-owned, in target\t05). RFC 2119 keywords; the capitals are load-bearing.
#
# A stand-in stands for agentspace.exe, agentspace-operator.exe, ssh, scp, aws or localswim.exe. It starts nothing, reads nothing but its answers
# folder and writes nothing but its calls log and counters. S02_STANDIN_DIR names the answers folder, whose files are
#   <tool>\<key>.out, <key>.err, <key>.exit, <key>.delay         one answer (stdout, stderr, exit code, seconds to wait first)
#   <tool>\<key>.<n>.out ...                                      the n-th call's answer; the last repeats
#   <tool>\<key>.<n>.provlog                                      lines appended to the file S02_STANDIN_PROVLOG on that call (agentspace only)
# S02_STANDIN_CALLS names a folder for calls.log (one line per call: the tool, what it was asked, never a value) and the counters.
# A call a stand-in does not recognise exits with 9 and says so, and a call with arguments that a script MUST NOT send exits with 2.

$script:SiDir = $env:S02_STANDIN_DIR
$script:SiCalls = $env:S02_STANDIN_CALLS

function Write-SiCall([string]$Text) {
  if (-not $script:SiCalls) { return }
  New-Item -ItemType Directory -Force -Path $script:SiCalls | Out-Null
  [IO.File]::AppendAllText((Join-Path $script:SiCalls 'calls.log'), $Text + "`n")
}

function Get-SiAnswerFile([string]$Tool, [string]$Key, [int]$N, [string]$Ext) {
  $base = Join-Path (Join-Path $script:SiDir $Tool) $Key
  $indexed = @(1..$N | ForEach-Object { $_ })
  for ($i = $N; $i -ge 1; $i--) {
    $f = "$base.$i.$Ext"
    if (Test-Path -LiteralPath $f -PathType Leaf) { return $f }
  }
  $f = "$base.$Ext"
  if (Test-Path -LiteralPath $f -PathType Leaf) { return $f }
  return $null
}

# The n-th call of (tool, key): increments its counter and answers from the files. Exits the process with the answer's exit code.
function Send-SiAnswer([string]$Tool, [string]$Key, [int]$Delay = -1) {
  $counters = if ($script:SiCalls) { Join-Path $script:SiCalls 'counters' } else { $null }
  $n = 1
  if ($counters) {
    New-Item -ItemType Directory -Force -Path $counters | Out-Null
    $c = Join-Path $counters "$Tool-$Key.n"
    if (Test-Path -LiteralPath $c) { $n = [int](Get-Content -LiteralPath $c -Raw).Trim() + 1 }
    [IO.File]::WriteAllText($c, [string]$n)
  }
  $utf8 = [Text.UTF8Encoding]::new($false)
  $delayFile = Get-SiAnswerFile $Tool $Key $n 'delay'
  $wait = $Delay
  if ($wait -lt 0) { $wait = 0; if ($delayFile) { $wait = [int]((Get-Content -LiteralPath $delayFile -Raw).Trim()) } }
  elseif ($delayFile) { $wait = [int]((Get-Content -LiteralPath $delayFile -Raw).Trim()) }
  if ($wait -gt 0) { Start-Sleep -Seconds $wait }
  $prov = Join-Path (Join-Path $script:SiDir $Tool) "$Key.$n.provlog"
  if ((Test-Path -LiteralPath $prov -PathType Leaf) -and $env:S02_STANDIN_PROVLOG) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $env:S02_STANDIN_PROVLOG) | Out-Null
    [IO.File]::AppendAllText($env:S02_STANDIN_PROVLOG, [IO.File]::ReadAllText($prov, $utf8))
  }
  $out = Get-SiAnswerFile $Tool $Key $n 'out'
  $err = Get-SiAnswerFile $Tool $Key $n 'err'
  $ex = Get-SiAnswerFile $Tool $Key $n 'exit'
  if (-not $out -and -not $err -and -not $ex) { Stop-SiUnknown $Tool "no answer file for the key $Key" }
  if ($out) { [Console]::Out.Write([IO.File]::ReadAllText($out, $utf8)); [Console]::Out.Flush() }
  if ($err) { [Console]::Error.Write([IO.File]::ReadAllText($err, $utf8)); [Console]::Error.Flush() }
  $code = 0
  if ($ex) { $code = [int]((Get-Content -LiteralPath $ex -Raw).Trim()) }
  exit $code
}

function Stop-SiUnknown([string]$Tool, [string]$What) {
  Write-SiCall "$Tool UNKNOWN $What"
  [Console]::Error.WriteLine("stand-in ${Tool}: no answer for this call: $What")
  exit 9
}

function Stop-SiRefused([string]$Tool, [string]$What) {
  Write-SiCall "$Tool REFUSED $What"
  [Console]::Error.WriteLine("stand-in ${Tool}: refused: $What")
  exit 2
}

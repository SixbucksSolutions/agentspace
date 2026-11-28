# Stand-in for ssh in E0.12-T05-S02's dry runs (drafter-owned). It reads the alias and the remote command from its arguments, picks the answer by the
# command's trailing `# s02:<key>` marker (or by -G, or by no command at all), and refuses a call without BatchMode=yes and ConnectTimeout=.
# A `sleep <n>` command waits n seconds (or the answer's .delay) first. The calls log gets the alias's kind (operator or lease), the key and
# whether ClearAllForwardings=yes was given, never the alias.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
$opts = @{}
$alias = $null
$rest = @()
$g = $false
for ($i = 0; $i -lt $a.Count; $i++) {
  $x = [string]$a[$i]
  if ($null -ne $alias) { $rest += $x; continue }
  if ($x -ceq '-o' -and $i + 1 -lt $a.Count) { $kv = [string]$a[$i + 1]; $eq = $kv.IndexOf('='); if ($eq -gt 0) { $opts[$kv.Substring(0, $eq)] = $kv.Substring($eq + 1) }; $i++; continue }
  if ($x -ceq '-G') { $g = $true; continue }
  if ($x.StartsWith('-')) { continue }
  $alias = $x
}
if ($null -eq $alias) { Stop-SiRefused 'ssh' 'no alias' }
$kind = if ($alias.StartsWith('agentspace-host.')) { 'operator' } else { 'lease' }
if ($alias -cmatch 'swimcoach|arm64' -or $alias -ceq 'agentspace' -or $alias -ceq 'agentbox') { Stop-SiRefused 'ssh' 'an arm64 host or the box was named' }
if ($g) { Write-SiCall "ssh -G $kind"; Send-SiAnswer 'ssh' $(if ($kind -ceq 'operator') { 'G-op' } else { 'G-lease' }) }
if ($opts['BatchMode'] -cne 'yes' -or -not $opts.ContainsKey('ConnectTimeout')) { Stop-SiRefused 'ssh' 'without BatchMode=yes and ConnectTimeout' }
$cmd = ($rest -join ' ')
$clear = ($opts['ClearAllForwardings'] -ceq 'yes')
if ($cmd -eq '') { Write-SiCall "ssh $kind login clearfwd=$clear"; Send-SiAnswer 'ssh' 'login' }
$m = [regex]::Match($cmd, '# s02:([A-Za-z0-9._-]+)\s*$')
if (-not $m.Success) { Stop-SiUnknown 'ssh' "a command with no # s02: marker (kind $kind)" }
$key = $m.Groups[1].Value
Write-SiCall "ssh $kind $key clearfwd=$clear"
if ($key -ceq 'sleep') {
  $s = [regex]::Match($cmd, 'sleep (\d+)')
  if (-not $s.Success) { Stop-SiRefused 'ssh' 'sleep with no seconds' }
  Send-SiAnswer 'ssh' 'sleep' ([int]$s.Groups[1].Value)
}
Send-SiAnswer 'ssh' $key

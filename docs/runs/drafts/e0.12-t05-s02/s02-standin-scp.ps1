# Stand-in for scp in E0.12-T05-S02's dry runs (drafter-owned). It accepts only a copy of one of the step's own host scripts (a local
# s02-host-*.ps1 that exists) to <alias>:.s02-host-*.ps1, with BatchMode=yes and ConnectTimeout=, and answers from its answers folder.
. (Join-Path $PSScriptRoot 's02-standin-lib.ps1')
$a = @($args)
$opts = @{}
$pos = @()
for ($i = 0; $i -lt $a.Count; $i++) {
  $x = [string]$a[$i]
  if ($x -ceq '-o' -and $i + 1 -lt $a.Count) { $kv = [string]$a[$i + 1]; $eq = $kv.IndexOf('='); if ($eq -gt 0) { $opts[$kv.Substring(0, $eq)] = $kv.Substring($eq + 1) }; $i++; continue }
  if ($x.StartsWith('-')) { continue }
  $pos += $x
}
if ($opts['BatchMode'] -cne 'yes' -or -not $opts.ContainsKey('ConnectTimeout')) { Stop-SiRefused 'scp' 'without BatchMode=yes and ConnectTimeout' }
if ($pos.Count -ne 2) { Stop-SiRefused 'scp' "$($pos.Count) path argument(s), expected 2" }
if ((Split-Path -Leaf $pos[0]) -cnotmatch '^s02-host-[A-Za-z0-9]+\.ps1$' -or -not (Test-Path -LiteralPath $pos[0] -PathType Leaf)) { Stop-SiRefused 'scp' 'the source is not one of the step''s own host scripts' }
$colon = $pos[1].IndexOf(':')
$dest = if ($colon -gt 0) { $pos[1].Substring($colon + 1) } else { '' }
if ($dest -cnotmatch '^\.s02-host-[A-Za-z0-9]+\.ps1$') { Stop-SiRefused 'scp' 'the destination is not .s02-host-<name>.ps1 in the home' }
$kind = if ($pos[1].StartsWith('agentspace-host.')) { 'operator' } else { 'lease' }
Write-SiCall "scp $kind $dest"
Send-SiAnswer 'scp' 'scp'

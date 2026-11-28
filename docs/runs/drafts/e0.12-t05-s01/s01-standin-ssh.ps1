# The dry runs' stand-in for ssh: `ssh -G <alias>`, `ssh [-o k=v]... <alias> <remote command>`. It answers as lease A's host
# would (s01-expected.md, step 213). RFC 2119 keywords; the capitals are load-bearing. S01_PLANT names ONE planted answer.
# `ssh -G` prints a user line and a proxycommand line: the step MUST print neither (the dry run's verdict checks it).
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 's01-standin-lib.ps1')
$plant = Get-SlPlant
$argv = @($args | ForEach-Object { [string]$_ })
$i = 0; $opts = @{}; $g = $false
while ($i -lt $argv.Count -and $argv[$i].StartsWith('-')) {
  if ($argv[$i] -ceq '-o') { $kv = $argv[$i + 1].Split('='); $opts[$kv[0]] = $kv[1]; $i += 2 }
  elseif ($argv[$i] -ceq '-G') { $g = $true; $i++ }
  else { $i++ }
}
$alias = $argv[$i]
$remote = (@($argv | Select-Object -Skip ($i + 1)) -join ' ')
if ($g) {
  if ($plant -ceq 'g-exit1') { [Console]::Error.WriteLine('ssh: Could not resolve hostname.'); exit 1 }
  [Console]::Out.WriteLine('user ubuntu')
  [Console]::Out.WriteLine(('hostname 203.0' + '.113.7'))
  [Console]::Out.WriteLine('port 22')
  if ($plant -cne 'g-no-proxy') { [Console]::Out.WriteLine('proxycommand C:\Tools\agentspace-ssh-proxy.exe %h') }
  exit 0
}
if (-not $opts.ContainsKey('BatchMode') -or $opts['BatchMode'] -cne 'yes') { [Console]::Error.WriteLine('stand-in: BatchMode=yes is missing'); exit 255 }
if (-not $opts.ContainsKey('ConnectTimeout') -or $opts['ConnectTimeout'] -cnotmatch '^\d+$') { [Console]::Error.WriteLine('stand-in: ConnectTimeout is missing'); exit 255 }
if ($remote -ceq 'true') {
  if ($plant -ceq 'true-fails') { [Console]::Error.WriteLine('ubuntu@host: Permission denied (publickey).'); exit 255 }
  exit 0
}
if ($remote -ceq 'uname -m') { [Console]::Out.WriteLine($(if ($plant -ceq 'uname-arm') { 'aarch64' } else { 'x86_64' })); exit 0 }
if ($remote -ceq 'ldd --version') {
  $v = switch ($plant) { 'ldd-242' { '2.42' } 'ldd-25' { '2.5' } 'ldd-3' { '3.0' } default { '2.43' } }
  if ($plant -ceq 'ldd-noversion') { [Console]::Out.WriteLine('ldd (Ubuntu GLIBC something) unknown'); exit 0 }
  [Console]::Out.WriteLine("ldd (Ubuntu GLIBC $v-2ubuntu2.7) $v")
  [Console]::Out.WriteLine('Copyright (C) 2026 Free Software Foundation, Inc.')
  exit 0
}
$versions = @{ node = 'v26.13.1'; uv = 'uv 0.13.9'; claude = '2.1.326 (Claude Code)'; codex = 'codex-cli 0.170.0'; gh = "gh version 2.105.0 (2026-11-01)`nhttps://github.com/cli/cli/releases/tag/v2.105.0"; fnm = 'fnm 1.38.1' }
$login = $false
$cmd = $remote
if ($remote -cmatch "^bash -lic '(.+)'$") { $login = $true; $cmd = $Matches[1] }
if ($cmd -cmatch '^(\w+) --version$') {
  $tool = $Matches[1]
  if (-not $versions.ContainsKey($tool)) { [Console]::Error.WriteLine("bash: ${tool}: command not found"); exit 127 }
  if ($plant -ceq "missing-$tool") { [Console]::Error.WriteLine("bash: ${tool}: command not found"); exit 127 }
  if ($plant -ceq "login-only-$tool" -and -not $login) { [Console]::Error.WriteLine("bash: ${tool}: command not found"); exit 127 }
  if ($plant -ceq "noversion-$tool") { [Console]::Out.WriteLine($tool); exit 0 }
  if ($plant -ceq "empty-$tool") { exit 0 }
  if ($plant -ceq "exit1-$tool") { [Console]::Out.WriteLine($versions[$tool]); exit 1 }
  [Console]::Out.WriteLine($versions[$tool])
  exit 0
}
[Console]::Error.WriteLine('stand-in: no answer for this remote command')
exit 255

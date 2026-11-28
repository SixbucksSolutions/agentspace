# Step 213: over lease A's alias: uname -m, ldd --version, each agent-linux tool's version. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing.
# The alias is read from doc-212.json and is never printed; `ssh -G`'s output is reduced to whether a proxycommand line
# exists (its `user` line, and the line itself, are never printed). One ssh per command, each with BatchMode and a
# ConnectTimeout. The tools are looked for as `ssh <alias> <tool> --version` first; when that fails, as
# `ssh <alias> bash -lic '<tool> --version'`, the interactive login shell lease-api.md:1815 promises the PATH of, and a tool
# found only that way is a NOTE (lease-api.md open question 6), not a finding.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '213' 'over lease A''s alias: x86_64, glibc 2.43 or newer, and each agent-linux tool answers'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $h = Test-S01Hours 'Before the ssh commands'
    if ($null -ne $h) { return $h }
    $f = Get-S01StatePath 'doc-212.json'
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { Write-S01 'FINDING: doc-212.json is not in the state folder; step 212 has not run.'; return 1 }
    $doc = ConvertFrom-S01Json ($script:S01Utf8.GetString([IO.File]::ReadAllBytes($f)).Trim())
    $alias = $null
    if ($doc -is [hashtable] -and $doc['ssh'] -is [hashtable] -and $doc['ssh']['host_alias'] -is [string]) { $alias = $doc['ssh']['host_alias'] }
    if (-not $alias -or $alias -cnotmatch '^agentspace-[a-z0-9][a-z0-9-]*-[0-9a-f]{8}$') { Write-S01 'FINDING: doc-212.json holds no usable host alias; the check ends here.'; return 1 }
    Write-S01 'Alias read from the state file: True (never printed)'

    # 1. The alias is proved first: a proxycommand line exists, and one real login works.
    $g = Invoke-S01Tool -Tool 'ssh' -Arguments @('-G', $alias) -TimeoutSec 60
    $hasProxy = $false
    if (-not $g.StartFailed -and -not $g.TimedOut -and $g.ExitCode -eq 0) { $hasProxy = (@($g.Stdout -split "\r?\n" | Where-Object { $_ -cmatch '^proxycommand\s+\S' }).Count -gt 0) }
    Write-S01 "ssh -G <alias>: exit $($g.ExitCode); a proxycommand line exists: $hasProxy (no other line is printed)"
    if ($g.StartFailed -or $g.ExitCode -ne 0) { $p.Add("ssh -G exited $($g.ExitCode), expected 0") }
    elseif (-not $hasProxy) { $p.Add('ssh -G shows no proxycommand line for the alias') }
    if ($p.Count -gt 0) { return (Complete-S01 $p) }
    $sshBase = @('-o', 'BatchMode=yes')
    $t = Invoke-S01Tool -Tool 'ssh' -Arguments ($sshBase + @('-o', 'ConnectTimeout=60', $alias, 'true')) -TimeoutSec 180
    Write-S01 "ssh <alias> true: exit $($t.ExitCode), stdout bytes $($t.StdoutBytes.Length)"
    if ($t.StartFailed -or $t.TimedOut -or $t.ExitCode -ne 0) {
      if ($t.StderrBytes.Length -gt 0) { Write-S01 ('ssh error (masked): ' + (Get-S01Excerpt $t)) }
      $p.Add("the real login (ssh <alias> true) exited $($t.ExitCode), expected 0; no command is run after it")
      return (Complete-S01 $p)
    }
    $remote = {
      param([string]$Cmd)
      Invoke-S01Tool -Tool 'ssh' -Arguments ($sshBase + @('-o', 'ConnectTimeout=30', $alias, $Cmd)) -TimeoutSec 150
    }

    # 2. uname -m
    $u = & $remote 'uname -m'
    $uname = $u.Stdout.Trim()
    Write-S01 "uname -m: exit $($u.ExitCode), prints '$uname'"
    if ($u.ExitCode -ne 0) { $p.Add("uname -m exited $($u.ExitCode)") }
    elseif ($uname -cne 'x86_64') { $p.Add("uname -m prints '$uname', expected 'x86_64'") }

    # 3. ldd --version, the first line, compared as numbers
    $d = & $remote 'ldd --version'
    $first = (@($d.Stdout -split "\r?\n" | Where-Object { $_ -ne '' }) | Select-Object -First 1)
    if ($null -eq $first) { $first = '' }
    Write-S01 "ldd --version: exit $($d.ExitCode), first line: $first"
    if ($d.ExitCode -ne 0) { $p.Add("ldd --version exited $($d.ExitCode)") }
    elseif ($first -cmatch '(\d+)\.(\d+)\s*$') {
      $major = [int]$Matches[1]; $minor = [int]$Matches[2]
      $ok = ($major -gt 2 -or ($major -eq 2 -and $minor -ge 43))
      Write-S01 "glibc $major.$minor is 2.43 or newer, compared as numbers: $ok"
      if (-not $ok) { $p.Add("glibc is $major.$minor, expected 2.43 or newer") }
    } else { $p.Add('the first line of ldd --version ends in no version number') }

    # 4. The tools (lease-api.md:1806-1810)
    $tools = @('node', 'uv', 'claude', 'codex', 'gh', 'fnm')
    $answered = 0
    foreach ($tool in $tools) {
      $form = 'direct'
      $r = & $remote "$tool --version"
      $ok = ($r.ExitCode -eq 0 -and $r.Stdout -cmatch '\d+\.\d+')
      $direct = $r.ExitCode
      if (-not $ok) {
        $form = 'login shell'
        $r = & $remote "bash -lic '$tool --version'"
        $ok = ($r.ExitCode -eq 0 -and $r.Stdout -cmatch '\d+\.\d+')
      }
      $line = (@($r.Stdout -split "\r?\n" | Where-Object { $_ -ne '' }) | Select-Object -First 1)
      if ($null -eq $line) { $line = '' }
      if ($line.Length -gt 100) { $line = $line.Substring(0, 100) + '...' }
      if ($ok) {
        $answered++
        Write-S01 "tool ${tool}: answered ($form form, exit 0): $line"
        if ($form -ceq 'login shell') { Write-S01 "  NOTE: ${tool} is found by an interactive login shell only (the direct form exited $direct), as lease-api.md:1815 promises; open question 6 stays open." }
      } else {
        Write-S01 "tool ${tool}: NOT answered (direct form exit $direct; login shell form exit $($r.ExitCode)): $line"
        $p.Add("the tool $tool does not answer --version with a version number")
      }
    }
    Write-S01 "Tools answering: $answered of $($tools.Count)"
    return (Complete-S01 $p)
  })

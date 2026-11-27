#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests box/windows/agentspace-idle-check.ps1 (E0.12-T03-S07, #0260; D11, D57, D78, D96), written from
# docs/operator.md, "The idle check on a Windows host" and "The tests' seams", and not from the script.
# This file holds D11's rule, the sessions (D78's worker rules, conhost.exe passed over), the settings
# file, the state files and the seam's refusals. The horizon's rules and #0192's four cases' Windows
# twins are test-agentspace-idle-horizon.ps1's.
#
# Seams (operator.md, The tests' seams): AGENTSPACE_IDLE_STATE, AGENTSPACE_IDLE_CONF and
# AGENTSPACE_IDLE_FACTS, set together. The clock is the real one, so each case sets its times relative
# to its own run. Nothing here starts shutdown, a service or a task: the script writes `poweroff` in the
# state folder instead. The script's folder is -ScriptDir (default: box/windows beside this folder), so a
# plant proof can point it at a copy.
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'agentspace-idle-check'
$scriptPath = Join-Path (Resolve-WtScriptDir $ScriptDir) ($scriptName + '.ps1')
$testHome = New-WtHome

try {
  Assert-WtScriptExists -Path $scriptPath
  $realBefore = Get-WtRealMark

  # --- D11's rule -----------------------------------------------------------------------------------
  Test-WtCase 'D11: An idle host 65 minutes past its last activity powers off, with its line' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (65 * 60 + 20)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for 65 min \(limit 60\); powering off$'
  }
  Test-WtCase 'D11: An idle host 30 minutes past its last activity stays up, with its line' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (30 * 60 + 20)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for 30 min of 60 \(CPU 0 s since last check\)$'
  }
  Test-WtCase 'D11: 60 minutes is enough to power off' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (60 * 60 + 20)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'D11: 59 minutes is not enough to power off' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (59 * 60 + 20)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for 59 min of 60 '
  }
  Test-WtCase 'D11: The limit is IDLE_MINUTES from the settings file' {
    $c = New-WtIdle -TestHome $testHome -Conf "IDLE_MINUTES=30`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=15`n"
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (35 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for 3[56] min \(limit 30\); powering off$'
  }
  Test-WtCase 'D11: A missing settings file gives the limit of 60 and the 6 CPU-seconds' {
    $c = New-WtIdle -TestHome $testHome -NoConf
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (50 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 70000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work: 7 s since last check$'
  }

  # --- The settings file is text -----------------------------------------------------------------------
  Test-WtCase 'Settings: BUSY_CPU_SECONDS from the file is the activity threshold' {
    $c = New-WtIdle -TestHome $testHome -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=20`nHORIZON_GRACE_MINUTES=15`n"
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 80000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNoStdoutLine -Result $r -Pattern '^Active:'
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min of 60 \(CPU 8 s since last check\)$'
  }
  Test-WtCase 'Settings: A line starting # is ignored' {
    $c = New-WtIdle -TestHome $testHome -Conf "# IDLE_MINUTES=5`nIDLE_MINUTES=45`nBUSY_CPU_SECONDS=6`n"
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (20 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for 2[01] min of 45 '
  }
  Test-WtCase 'Settings: The file is read as text and never run' {
    $c = New-WtIdle -TestHome $testHome
    $marker = Join-Path $c.Dir 'ran.txt'
    $conf = "& { Set-Content -LiteralPath '$marker' -Value ran }`n" +
      "Set-Content -LiteralPath '$marker' -Value ran`n" +
      "BUSY_CPU_SECONDS=`$(Set-Content -LiteralPath '$marker' -Value ran)`n" +
      "IDLE_MINUTES=30`nHORIZON_GRACE_MINUTES=15`n"
    Write-WtText -TestHome $testHome -Path $c.ConfPath -Text $conf
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (40 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-Wt (-not (Test-Path -LiteralPath $marker)) 'The settings file MUST NOT be run: a command in it ran'
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min \(limit 30\); powering off$'
  }

  # --- Sessions (D78 on Windows) ------------------------------------------------------------------------
  Test-WtCase 'Session: An established SSH connection on port 22 keeps the host up' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ ssh_connections = 1 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: SSH session open$'
  }
  Test-WtCase 'Session: A worker holding a connection to sshd is a Session Manager session' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $true; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Session: A worker with a child that is not conhost.exe is a person''s shell' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @('cmd.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Session: A conhost.exe child among others does not stop the shell counting' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @('conhost.exe', 'pwsh.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Session: A worker whose only child is conhost.exe is not a session' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @('conhost.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtNoStdoutLine -Result $r -Pattern '^Active: Session Manager'
  }
  Test-WtCase 'Session: A worker with no connection and no child is not a session' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Session: Two bare workers do not count' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @() }, @{ sshd_connection = $false; children = @('conhost.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Session: Every worker is looked at, not only the first' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @() }, @{ sshd_connection = $false; children = @('conhost.exe') }, @{ sshd_connection = $true; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Session: A patch run under way keeps the host up' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ patch_running = $true }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Patching$'
  }
  Test-WtCase 'Session: An SSH session is named before a Session Manager one, a patch run and CPU work' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ ssh_connections = 3; patch_running = $true; cpu_busy_ticks = $script:WtBaseTicks + 90000000; workers = @(@{ sshd_connection = $true; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtStdoutLine -Result $r -Pattern '^Active: SSH session open$'
  }
  Test-WtCase 'Session: A Session Manager session is named before a patch run and CPU work' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ patch_running = $true; cpu_busy_ticks = $script:WtBaseTicks + 90000000; workers = @(@{ sshd_connection = $true; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Session: A patch run is named before CPU work' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ patch_running = $true; cpu_busy_ticks = $script:WtBaseTicks + 90000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Patching$'
  }

  # --- CPU work -----------------------------------------------------------------------------------------
  Test-WtCase 'CPU: 8 seconds of work since the last check is activity, with its line' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 85000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work: 8 s since last check$'
  }
  Test-WtCase 'CPU: Exactly 6 seconds of work is activity' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 60000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work: 6 s since last check$'
  }
  Test-WtCase 'CPU: 59999999 ticks round down to 5 seconds, which is not activity' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 59999999 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'CPU: Seconds are the tick difference over 10000000, rounded down' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (10 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 49999999 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min of 60 \(CPU 4 s since last check\)$'
  }
  Test-WtCase 'CPU: The counter is recorded in busy for the next run' {
    $c = New-WtIdle -TestHome $testHome
    $ticks = $script:WtBaseTicks + 123456789
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $ticks }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    $busy = Join-Path $c.State 'busy'
    Assert-Wt (Test-Path -LiteralPath $busy -PathType Leaf) 'busy MUST be written'
    Assert-WtEqual ((Read-WtText -Path $busy).Trim()) ([string]$ticks) 'busy MUST hold the counter as a decimal integer'
  }
  Test-WtCase 'CPU: A second run measures from the counter the first recorded' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (10 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 200000000 }
    [void](Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 200000000 + 30000000 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min of 60 \(CPU 3 s since last check\)$'
  }

  # --- The state files -----------------------------------------------------------------------------------
  Test-WtCase 'State: An active run records the time as the last activity' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (50 * 60)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ ssh_connections = 1 }
    [void](Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath)
    $la = Join-Path $c.State 'last-active'
    $t = [long]((Read-WtText -Path $la).Trim())
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'last-active MUST hold the run''s time as Unix seconds after an active run'
  }
  Test-WtCase 'State: A run with no activity leaves the last activity as it was' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (50 * 60)
    $la = Join-Path $c.State 'last-active'
    $before = Read-WtBytes -Path $la
    [void](Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath)
    Assert-WtBytes -Actual (Read-WtBytes -Path $la) -Expected $before -What 'last-active'
  }
  Test-WtCase 'State: With no last-active the first run starts the window and does not power off' {
    $c = New-WtIdle -TestHome $testHome
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    $la = Join-Path $c.State 'last-active'
    Assert-Wt (Test-Path -LiteralPath $la -PathType Leaf) 'last-active MUST be written by the first run of a boot'
    $t = [long]((Read-WtText -Path $la).Trim())
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'The last activity MUST start as the first run''s time'
  }
  Test-WtCase 'State: A last-active last written before this boot is missing, not 120 idle minutes' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $c.Now - 3600 }
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (120 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'State: A last-active written after this boot is read (the control for the old one)' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $c.Now - 9000 }
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (120 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'State: The check writes nothing to the horizon file' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now + 3600))
    $path = Join-Path $c.State 'lease-horizon'
    $bytes = Read-WtBytes -Path $path
    $mtime = (Get-Item -LiteralPath $path).LastWriteTimeUtc
    [void](Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath)
    Assert-WtBytes -Actual (Read-WtBytes -Path $path) -Expected $bytes -What 'lease-horizon'
    Assert-Wt ((Get-Item -LiteralPath $path).LastWriteTimeUtc -eq $mtime) 'lease-horizon MUST NOT be rewritten'
  }

  # --- Streams and the log ---------------------------------------------------------------------------------
  Test-WtCase 'Output: A run decides once and prints one line to stdout' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (10 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    $lines = @(Get-WtLines -Text $r.Stdout)
    Assert-WtEqual $lines.Count 1 'Stdout lines'
  }
  Test-WtCase 'Output: A power-off is not an error: the check exits 0' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtEqual $r.ExitCode 0 'Exit code'
  }
  Test-WtCase 'Output: The poweroff stand-in is one Unix time and one LF' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    [void](Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath)
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'poweroff')
    Assert-Wt ($null -ne $t) 'poweroff MUST be written'
  }

  # --- The seam is all or nothing -----------------------------------------------------------------------------
  $idleNames = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_IDLE_CONF', 'AGENTSPACE_IDLE_FACTS')
  foreach ($subset in @(@(0), @(1), @(2), @(0, 1), @(0, 2), @(1, 2))) {
    $label = ($subset | ForEach-Object { $idleNames[$_] -replace '^AGENTSPACE_IDLE_', '' }) -join ' and '
    $subsetCopy = @($subset)
    Test-WtCase ("Seam: Only $label set is refused, nothing done") {
      $c = New-WtIdle -TestHome $testHome
      Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (90 * 60)
      $all = @{ AGENTSPACE_IDLE_STATE = $c.State; AGENTSPACE_IDLE_CONF = $c.ConfPath; AGENTSPACE_IDLE_FACTS = $c.FactsPath }
      $seam = @{ AGENTSPACE_IDLE_STATE = $null; AGENTSPACE_IDLE_CONF = $null; AGENTSPACE_IDLE_FACTS = $null }
      foreach ($i in $subsetCopy) { $seam[$idleNames[$i]] = $all[$idleNames[$i]] }
      $before = Get-WtSnapshot -Path $c.Dir
      $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath -Seam $seam
      Assert-WtSeamRefusal -Result $r -ScriptName $scriptName -Variables $idleNames
      Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder (state, settings, facts)'
    }
  }
  Test-WtCase 'Seam: A facts file that is missing is refused, nothing done' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (90 * 60)
    Remove-WtFile -TestHome $testHome -Path $c.FactsPath
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains("$scriptName`: ") -and $r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-Wt ($r.Stderr.Contains('facts.json')) 'The message MUST name the facts file'
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: A facts file that is not valid JSON is refused, nothing done' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (90 * 60)
    Write-WtText -TestHome $testHome -Path $c.FactsPath -Text '{ "boot": 12, '
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains("$scriptName`: ") -and $r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests the lease horizon's rules of box/windows/agentspace-idle-check.ps1 (E0.12-T03-S07, #0260; D57,
# D63, D78, D96; #0192's four cases with their Windows twins), written from docs/operator.md, "The idle
# check on a Windows host", "The idle check and the lease horizon" and "The tests' seams", and not from
# the script. D11's rule, the sessions, the settings file and the seam's refusals are
# test-agentspace-idle-check.ps1's.
#
# Seams (operator.md, The tests' seams): AGENTSPACE_IDLE_STATE, AGENTSPACE_IDLE_CONF and
# AGENTSPACE_IDLE_FACTS. The clock is the real one, so each case sets its horizon relative to its own
# run, and no case compares clock times across two runs. A case that needs a horizon "past" puts it
# well past (25 minutes against the default 15-minute grace); a boundary case sits 20 seconds on one
# side of it, which a run of one process fits inside. The script's folder is -ScriptDir (default:
# box/windows beside this folder), so a plant proof can point it at a copy.
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'agentspace-idle-check'
$scriptPath = Join-Path (Resolve-WtScriptDir $ScriptDir) ($scriptName + '.ps1')
$testHome = New-WtHome

# A case with CPU work since the last check (8 seconds), so that D11's rule alone keeps the host up and
# only the horizon rule can power it off.
function New-WtBusyCase {
  param([AllowEmptyString()][string]$Conf = $script:WtIdleConf, [switch]$NoConf)
  if ($NoConf) { $c = New-WtIdle -TestHome $testHome -NoConf } else { $c = New-WtIdle -TestHome $testHome -Conf $Conf }
  Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ cpu_busy_ticks = $script:WtBaseTicks + 80000000 }
  return $c
}

function Get-WtPastLine {
  param([long]$Horizon, [int]$Grace = 15)
  return ('Past lease horizon {0} plus {1} min with no session; powering off' -f (Format-WtClock -Unix $Horizon), $Grace)
}

try {
  Assert-WtScriptExists -Path $scriptPath
  $realBefore = Get-WtRealMark

  # --- The horizon rule (D57) ----------------------------------------------------------------------------
  Test-WtCase 'Horizon: Past the horizon plus the grace, with no session, the host powers off whatever its CPU does' {
    $c = New-WtBusyCase
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }
  Test-WtCase 'Horizon: The line shows the horizon as h:mm and am or pm, with no leading zero, in the laptop''s zone' {
    # The latest past time of the form 1:00 to 9:09, am or pm, so that both the unpadded hour and the
    # padded minute are in the line.
    $c = New-WtBusyCase
    $h = $c.Now - 1800
    while ($true) {
      $t = [DateTimeOffset]::FromUnixTimeSeconds($h).ToLocalTime()
      $h12 = $t.Hour % 12
      if ($h12 -ge 1 -and $h12 -le 9 -and $t.Minute -lt 10) { break }
      $h = $h - 60
    }
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $h - 7200 }
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    $want = Get-WtPastLine -Horizon $h
    Assert-Wt ($want -match '^Past lease horizon [1-9]:0\d(am|pm) plus 15 ') ("The expected line is not of the form being tested: {0}" -f $want)
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape($want) + '$')
  }
  Test-WtCase 'Horizon: The grace in the line is HORIZON_GRACE_MINUTES from the settings file' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=20`n"
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h -Grace 20)) + '$')
  }
  Test-WtCase 'Horizon: Inside the grace the host stays up' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 10 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work: 8 s since last check$'
  }
  Test-WtCase 'Horizon: A horizon in the future leaves D11''s rule alone, CPU busy' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now + 3600))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work:'
  }
  Test-WtCase 'Horizon: A horizon in the future does not stop D11''s idle power-off' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now + 3600))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min \(limit 60\); powering off$'
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }

  # --- What holds the horizon rule off ---------------------------------------------------------------------
  Test-WtCase 'Hold: An SSH session holds off the horizon''s power-off' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ ssh_connections = 1 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: SSH session open$'
  }
  Test-WtCase 'Hold: A Session Manager worker holding a connection holds off the horizon''s power-off' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $true; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Session Manager session open$'
  }
  Test-WtCase 'Hold: A Session Manager shell holds off the horizon''s power-off' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @('powershell.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Hold: A patch run under way holds off the horizon''s power-off' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ patch_running = $true }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: Patching$'
  }
  Test-WtCase 'Hold: A worker with no connection and no shell does not hold it off (#0195)' {
    $c = New-WtBusyCase
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @() }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }
  Test-WtCase 'Hold: A worker whose only child is conhost.exe does not hold it off (#0195)' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ workers = @(@{ sshd_connection = $false; children = @('conhost.exe') }) }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }

  # --- #0192 case 1 and its Windows twin: a CR in the horizon's line ------------------------------------------
  Test-WtCase '0192-1: A horizon ending CR LF is no horizon, and D11''s rule keeps a busy host up' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`r`n" -f ($c.Now - 25 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work:'
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }
  Test-WtCase '0192-1: A horizon ending CR LF leaves D11''s idle power-off with its own line' {
    $c = New-WtIdle -TestHome $testHome
    Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`r`n" -f ($c.Now - 25 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min \(limit 60\); powering off$'
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }
  Test-WtCase '0192-1: A horizon ending a bare CR is no horizon' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`r" -f ($c.Now - 25 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }
  Test-WtCase '0192-1: A CR anywhere in the first line is no horizon' {
    $c = New-WtBusyCase
    $digits = [string]($c.Now - 25 * 60)
    Set-WtHorizon -TestHome $testHome -Case $c -Text ($digits.Substring(0, 4) + "`r" + $digits.Substring(4) + "`n")
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase '0192-1: The control, a horizon ending LF alone, is used' {
    $c = New-WtBusyCase
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase '0192-1: A horizon with no line ending at all is used' {
    $c = New-WtBusyCase
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ([string]$h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }

  # --- #0192 case 2: the horizon's length ------------------------------------------------------------------------
  Test-WtCase '0192-2: A 16-digit horizon is no horizon' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text "0000000000000001`n"
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }
  Test-WtCase '0192-2: The control, a 15-digit horizon, is used' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text "000000000000001`n"
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon 1)) + '$')
  }

  # --- #0192 case 3: the boundary ----------------------------------------------------------------------------------
  Test-WtCase '0192-3: 20 seconds before the horizon plus the grace, D11''s rule applies' {
    $c = New-WtBusyCase
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now + 20 - 15 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work:'
  }
  Test-WtCase '0192-3: 20 seconds after the horizon plus the grace, the horizon rule applies' {
    $c = New-WtBusyCase
    $h = $c.Now - 20 - 15 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }

  # --- #0192 case 4: the grace's default ---------------------------------------------------------------------------
  Test-WtCase '0192-4: With HORIZON_GRACE_MINUTES unset, 14 minutes past the horizon the host stays up' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`n"
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 14 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase '0192-4: With HORIZON_GRACE_MINUTES unset, 16 minutes past the horizon the host powers off with 15' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`n"
    $h = $c.Now - 16 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }
  Test-WtCase '0192-4: With no settings file, 14 minutes past the horizon the host stays up' {
    $c = New-WtBusyCase -NoConf
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 14 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase '0192-4: With no settings file, 16 minutes past the horizon the host powers off with 15' {
    $c = New-WtBusyCase -NoConf
    $h = $c.Now - 16 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
  }

  # --- An invalid grace ---------------------------------------------------------------------------------------------
  foreach ($bad in @('abc', '', '1234567', '-5', '1.5', '10 min')) {
    $badCopy = $bad
    Test-WtCase "Grace: A HORIZON_GRACE_MINUTES of '$bad' becomes 15, with the warning on stderr" {
      $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=$badCopy`n"
      $h = $c.Now - 20 * 60
      Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
      $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
      Assert-WtPoweredOff -Case $c -Result $r
      Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h)) + '$')
      Assert-Wt ($r.Stderr.Contains('Invalid HORIZON_GRACE_MINUTES in ') -and $r.Stderr.Contains('; using 15')) ("stderr MUST hold 'Invalid HORIZON_GRACE_MINUTES in <conf>; using 15': {0}" -f $r.Stderr)
      Assert-Wt ($r.Stderr.Contains($c.ConfPath)) 'The warning MUST name the settings file by its path'
    }
  }
  Test-WtCase 'Grace: An invalid grace and a horizon 10 minutes past: 15 applies, so the host stays up' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=abc`n"
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 10 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Grace: A grace of 1 digit is valid and used' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=7`n"
    $h = $c.Now - 10 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern ('^' + [regex]::Escape((Get-WtPastLine -Horizon $h -Grace 7)) + '$')
    Assert-Wt (-not $r.Stderr.Contains('Invalid HORIZON_GRACE_MINUTES')) 'A valid grace MUST NOT warn'
  }
  Test-WtCase 'Grace: A grace of 6 digits is valid and used' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=999999`n"
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 600 * 60))
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-Wt (-not $r.Stderr.Contains('Invalid HORIZON_GRACE_MINUTES')) 'A valid grace MUST NOT warn'
  }
  Test-WtCase 'Grace: With no usable horizon an invalid grace is not read, so no warning' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=abc`n"
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-Wt (-not $r.Stderr.Contains('Invalid HORIZON_GRACE_MINUTES')) ("No horizon: the grace MUST NOT be read or warned about: {0}" -f $r.Stderr)
  }
  Test-WtCase 'Grace: With a session holding the host an invalid grace is not read, so no warning' {
    $c = New-WtBusyCase -Conf "IDLE_MINUTES=60`nBUSY_CPU_SECONDS=6`nHORIZON_GRACE_MINUTES=abc`n"
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60))
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ ssh_connections = 1 }
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-Wt (-not $r.Stderr.Contains('Invalid HORIZON_GRACE_MINUTES')) ("A session holds the host: the grace MUST NOT be read or warned about: {0}" -f $r.Stderr)
  }

  # --- What is no horizon (D11 alone) --------------------------------------------------------------------------------
  Test-WtCase 'Parse: No horizon file keeps D11''s rule, CPU busy' {
    $c = New-WtBusyCase
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work:'
  }
  $junk = [ordered]@{
    'garbage' = "not-a-time`n"
    'an empty file' = ''
    'an empty first line' = ("`n{0}`n" -f 1764000000)
    'a plus sign' = "+1764000000`n"
    'a minus sign' = "-1764000000`n"
    'a leading space' = " 1764000000`n"
    'a trailing space' = "1764000000 `n"
    'a decimal point' = "1764000000.5`n"
  }
  foreach ($label in $junk.Keys) {
    $text = $junk[$label]
    $labelCopy = $label
    Test-WtCase "Parse: A horizon file holding $labelCopy is no horizon, and D11's rule applies to an idle host" {
      $c = New-WtIdle -TestHome $testHome
      Set-WtLastActive -TestHome $testHome -Case $c -SecondsAgo (70 * 60)
      Set-WtHorizon -TestHome $testHome -Case $c -Text $text
      $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
      Assert-WtPoweredOff -Case $c -Result $r
      Assert-WtStdoutLine -Result $r -Pattern '^Idle for \d+ min \(limit 60\); powering off$'
      Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
    }
  }
  Test-WtCase 'Parse: A folder at the horizon path is no horizon' {
    $c = New-WtBusyCase
    New-WtFolder -TestHome $testHome -Path (Join-Path $c.State 'lease-horizon') | Out-Null
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtStdoutLine -Result $r -Pattern '^Active: CPU work:'
  }
  Test-WtCase 'Parse: Only the first line is read: a usable first line then more lines is a horizon' {
    $c = New-WtBusyCase
    $h = $c.Now - 25 * 60
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`nthis is not read`n" -f $h)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Parse: A horizon last written before this boot is no horizon' {
    $c = New-WtBusyCase
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $c.Now - 3600 }
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60)) -WrittenUnix ($c.Now - 7200)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtNotPoweredOff -Case $c -Result $r
    Assert-WtNoStdoutLine -Result $r -Pattern '^Past lease horizon'
  }
  Test-WtCase 'Parse: A horizon last written after this boot is used (the control for the old one)' {
    $c = New-WtBusyCase
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $c.Now - 3600 }
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60)) -WrittenUnix ($c.Now - 1800)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }
  Test-WtCase 'Parse: A horizon last written exactly at this boot''s start is used' {
    $c = New-WtBusyCase
    Set-WtIdleFacts -TestHome $testHome -Case $c -Change @{ boot = $c.Now - 3600 }
    Set-WtHorizon -TestHome $testHome -Case $c -Text ("{0}`n" -f ($c.Now - 25 * 60)) -WrittenUnix ($c.Now - 3600)
    $r = Invoke-WtIdle -TestHome $testHome -Case $c -ScriptPath $scriptPath
    Assert-WtPoweredOff -Case $c -Result $r
  }

  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

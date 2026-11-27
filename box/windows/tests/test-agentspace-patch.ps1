#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests box/windows/agentspace-patch.ps1 (E0.12-T03-S07, #0260; D96), written from docs/operator.md,
# "The patch run and its restart rule" and "The tests' seams", and not from the script: its markers, its
# log, and above all the restart rule (a patch run MUST NOT restart the host while a login session is
# open, restarts only before the host's first session after its start, and never more than once in 15
# minutes).
#
# Seams (operator.md, The tests' seams): AGENTSPACE_PATCH_STATE, the folder of the markers, of
# last-patch-reboot and of patch.log, and AGENTSPACE_PATCH_FACTS, a JSON file with boot, result (ok or
# failed), restart_needed, logins_since_boot and workers. Under the seam the script calls no Windows
# Update API, starts no tools and writes `restart` in the state folder in place of shutdown.exe /r. The
# clock is the real one. The script's folder is -ScriptDir (default: box/windows beside this folder).
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'agentspace-patch'
$scriptPath = Join-Path (Resolve-WtScriptDir $ScriptDir) ($scriptName + '.ps1')
$testHome = New-WtHome

# A case: a state folder and a facts file for a run that finds the host booted 3 hours ago, with no
# login, no worker, no restart needed and the update run ok. $Change replaces facts.
function New-WtPatch {
  param([hashtable]$Change = @{})
  $dir = New-WtCase -TestHome $testHome
  $now = Get-WtUnixNow
  $c = [pscustomobject]@{
    Dir = $dir
    State = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'state')
    FactsPath = Join-Path $dir 'facts.json'
    Now = $now
    Facts = [ordered]@{ boot = $now - 10800; result = 'ok'; restart_needed = $false; logins_since_boot = 0; workers = 0 }
  }
  foreach ($k in $Change.Keys) { $c.Facts[$k] = $Change[$k] }
  Write-WtJson -TestHome $testHome -Path $c.FactsPath -Object $c.Facts
  return $c
}

function Invoke-WtPatch {
  param([Parameter(Mandatory)]$Case, [hashtable]$Seam = $null)
  if ($null -eq $Seam) { $Seam = @{ AGENTSPACE_PATCH_STATE = $Case.State; AGENTSPACE_PATCH_FACTS = $Case.FactsPath } }
  return Invoke-WtScript -TestHome $testHome -ScriptPath $scriptPath -Seam $Seam
}

function Test-WtMarker { param($Case, [string]$Name) return (Test-Path -LiteralPath (Join-Path $Case.State $Name) -PathType Leaf) }

# What a restart leaves: `restart` (the shutdown stand-in) and last-patch-reboot, and no marker.
function Assert-WtRestarted {
  param($Case, $Result)
  Assert-WtEqual $Result.ExitCode 0 'Exit code'
  Assert-Wt (Test-WtMarker $Case 'restart') ("The host MUST be restarted (restart written). {0}" -f (Format-WtRun $Result))
  $t = Read-WtUnixLine -Path (Join-Path $Case.State 'restart')
  Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'restart MUST hold the time as Unix seconds'
}
function Assert-WtNotRestarted {
  param($Case, $Result)
  Assert-Wt (-not (Test-WtMarker $Case 'restart')) ("The host MUST NOT be restarted. {0}" -f (Format-WtRun $Result))
}

try {
  Assert-WtScriptExists -Path $scriptPath
  $realBefore = Get-WtRealMark

  # --- A run that needs no restart -------------------------------------------------------------------------
  Test-WtCase 'Run: A run that installs nothing writes patched, holding the time as Unix seconds, and exits 0' {
    $c = New-WtPatch
    $r = Invoke-WtPatch -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'patched')
    Assert-Wt ($null -ne $t) ("patched MUST be written. {0}" -f (Format-WtRun $r))
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'patched MUST hold the time as Unix seconds'
    Assert-Wt (-not (Test-WtMarker $c 'patch-failed')) 'patch-failed MUST NOT be written by an ok run'
    Assert-WtNotRestarted -Case $c -Result $r
  }
  Test-WtCase 'Run: A run starts by removing both markers, so an old patch-failed does not outlive an ok run' {
    $c = New-WtPatch
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patch-failed') -Text ("{0}`n" -f ($c.Now - 600)) -WrittenUnix ($c.Now - 600)
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patched') -Text ("{0}`n" -f ($c.Now - 900)) -WrittenUnix ($c.Now - 900)
    $r = Invoke-WtPatch -Case $c
    Assert-Wt (-not (Test-WtMarker $c 'patch-failed')) 'The old patch-failed MUST be removed'
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'patched')
    Assert-Wt ($t -gt ($c.Now - 300)) 'patched MUST be the new run''s, not the old one'
  }
  Test-WtCase 'Run: A run logs to patch.log in the state folder' {
    $c = New-WtPatch
    [void](Invoke-WtPatch -Case $c)
    $log = Join-Path $c.State 'patch.log'
    Assert-Wt (Test-Path -LiteralPath $log -PathType Leaf) 'patch.log MUST be written in the state folder'
    Assert-Wt ((Read-WtText -Path $log).Trim().Length -gt 0) 'patch.log MUST hold lines'
  }
  Test-WtCase 'Run: A log line holds the time in UTC' {
    $c = New-WtPatch
    [void](Invoke-WtPatch -Case $c)
    $first = (Get-WtLines -Text (Read-WtText -Path (Join-Path $c.State 'patch.log')))[0]
    Assert-Wt ($first -match '\d{4}-\d{2}-\d{2}') ("A log line MUST start with the date in UTC: {0}" -f $first)
  }
  Test-WtCase 'Run: A log that grew past 1 MiB is renamed over patch.log.1 before the next line' {
    $c = New-WtPatch
    $log = Join-Path $c.State 'patch.log'
    $big = [byte[]]::new(1100000)
    [Array]::Fill($big, [byte]97)
    Write-WtBytes -TestHome $testHome -Path $log -Bytes $big
    [void](Invoke-WtPatch -Case $c)
    $rolled = Join-Path $c.State 'patch.log.1'
    Assert-Wt (Test-Path -LiteralPath $rolled -PathType Leaf) 'patch.log.1 MUST hold the old log'
    Assert-WtEqual ((Get-Item -LiteralPath $rolled).Length) 1100000 'patch.log.1 size'
    Assert-Wt ((Get-Item -LiteralPath $log).Length -lt 100000) 'patch.log MUST start again small'
  }

  # --- A failed run -----------------------------------------------------------------------------------------------
  Test-WtCase 'Failure: A failed search, download or install writes patch-failed, no patched, and exits 1' {
    $c = New-WtPatch -Change @{ result = 'failed' }
    $r = Invoke-WtPatch -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'patch-failed')
    Assert-Wt ($null -ne $t) ("patch-failed MUST be written. {0}" -f (Format-WtRun $r))
    Assert-Wt (-not (Test-WtMarker $c 'patched')) 'patched MUST NOT be written by a failed run'
  }
  Test-WtCase 'Failure: A failed run that would need a restart makes none' {
    $c = New-WtPatch -Change @{ result = 'failed'; restart_needed = $true }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-Wt (Test-WtMarker $c 'patch-failed') 'patch-failed MUST be written'
  }
  Test-WtCase 'Failure: A failed run removes an old patched marker' {
    $c = New-WtPatch -Change @{ result = 'failed' }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patched') -Text ("{0}`n" -f ($c.Now - 900)) -WrittenUnix ($c.Now - 900)
    [void](Invoke-WtPatch -Case $c)
    Assert-Wt (-not (Test-WtMarker $c 'patched')) 'The old patched MUST be removed at the start of the run'
  }

  # --- The restart rule ----------------------------------------------------------------------------------------------
  Test-WtCase 'Restart: Needed, no login since boot, no worker, no earlier restart: the host restarts, with no marker' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    $r = Invoke-WtPatch -Case $c
    Assert-WtRestarted -Case $c -Result $r
    Assert-Wt (-not (Test-WtMarker $c 'patched')) 'A run that restarts writes no marker: the next boot''s run writes it'
    Assert-Wt (-not (Test-WtMarker $c 'patch-failed')) 'A run that restarts is not a failure'
  }
  Test-WtCase 'Restart: A restart writes last-patch-reboot, holding the time as Unix seconds' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    [void](Invoke-WtPatch -Case $c)
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'last-patch-reboot')
    Assert-Wt ($null -ne $t) 'last-patch-reboot MUST be written when the host is restarted'
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'last-patch-reboot MUST hold the restart''s time'
  }
  Test-WtCase 'Restart: Not needed, the host does not restart however quiet it is' {
    $c = New-WtPatch -Change @{ restart_needed = $false }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-Wt (-not (Test-WtMarker $c 'last-patch-reboot')) 'last-patch-reboot MUST NOT be written with no restart'
  }
  Test-WtCase 'Restart: A login since this boot keeps the restart for the next boot, and the run goes on to patched' {
    $c = New-WtPatch -Change @{ restart_needed = $true; logins_since_boot = 1 }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    Assert-Wt (Test-WtMarker $c 'patched') 'The run MUST go on to write patched'
    Assert-Wt (-not (Test-WtMarker $c 'last-patch-reboot')) 'last-patch-reboot MUST NOT be written with no restart'
  }
  Test-WtCase 'Restart: Many logins since this boot keep the restart too' {
    $c = New-WtPatch -Change @{ restart_needed = $true; logins_since_boot = 7 }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
  }
  Test-WtCase 'Restart: The log says the restart waits for the next boot' {
    $c = New-WtPatch -Change @{ restart_needed = $true; logins_since_boot = 1 }
    [void](Invoke-WtPatch -Case $c)
    $log = Read-WtText -Path (Join-Path $c.State 'patch.log')
    Assert-Wt ($log -match '(?i)next boot') ("patch.log MUST say the restart waits for the next boot: {0}" -f $log)
  }
  Test-WtCase 'Restart: A Session Manager worker running keeps the restart for the next boot' {
    $c = New-WtPatch -Change @{ restart_needed = $true; workers = 1 }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-Wt (Test-WtMarker $c 'patched') 'The run MUST go on to write patched'
  }
  Test-WtCase 'Restart: Two workers keep it too' {
    $c = New-WtPatch -Change @{ restart_needed = $true; workers = 2 }
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
  }
  Test-WtCase 'Restart: A restart 10 minutes ago keeps this one for the next boot' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 600)) -WrittenUnix ($c.Now - 600)
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-Wt (Test-WtMarker $c 'patched') 'The run MUST go on to write patched'
  }
  Test-WtCase 'Restart: A restart 14 minutes ago keeps this one for the next boot' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 14 * 60)) -WrittenUnix ($c.Now - 14 * 60)
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
  }
  Test-WtCase 'Restart: A restart 16 minutes ago lets this one happen' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 16 * 60)) -WrittenUnix ($c.Now - 16 * 60)
    $r = Invoke-WtPatch -Case $c
    Assert-WtRestarted -Case $c -Result $r
    $t = Read-WtUnixLine -Path (Join-Path $c.State 'last-patch-reboot')
    Assert-Wt ($t -gt ($c.Now - 300)) 'last-patch-reboot MUST name the new restart'
  }
  Test-WtCase 'Restart: A restart a day ago lets this one happen' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 86400)) -WrittenUnix ($c.Now - 86400)
    $r = Invoke-WtPatch -Case $c
    Assert-WtRestarted -Case $c -Result $r
  }
  Test-WtCase 'Restart: The 15 minutes count across boots, so an update asking again at every start does not loop' {
    # The host was restarted by the last run 3 minutes ago, and booted 2 minutes ago: last-patch-reboot
    # was last written before this boot, and still holds the restart off.
    $c = New-WtPatch -Change @{ restart_needed = $true; boot = (Get-WtUnixNow) - 120 }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 180)) -WrittenUnix ($c.Now - 180)
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
    Assert-Wt (Test-WtMarker $c 'patched') 'The run MUST go on to write patched'
    $kept = Read-WtUnixLine -Path (Join-Path $c.State 'last-patch-reboot')
    Assert-WtEqual $kept ($c.Now - 180) 'last-patch-reboot MUST be left as it was'
  }
  Test-WtCase 'Restart: A restart also removes both markers it found at its start' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patched') -Text ("{0}`n" -f ($c.Now - 900)) -WrittenUnix ($c.Now - 900)
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patch-failed') -Text ("{0}`n" -f ($c.Now - 900)) -WrittenUnix ($c.Now - 900)
    $r = Invoke-WtPatch -Case $c
    Assert-WtRestarted -Case $c -Result $r
    Assert-Wt (-not (Test-WtMarker $c 'patched')) 'No marker after a restart: patched MUST be gone'
    Assert-Wt (-not (Test-WtMarker $c 'patch-failed')) 'No marker after a restart: patch-failed MUST be gone'
  }
  Test-WtCase 'Restart: The restart is one line, a Unix time and one LF, in the form of poweroff' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    [void](Invoke-WtPatch -Case $c)
    Assert-Wt ($null -ne (Read-WtUnixLine -Path (Join-Path $c.State 'restart'))) 'restart MUST be written'
  }
  Test-WtCase 'Restart: The restart rule needs all three conditions: a login and a worker and a recent restart each hold it' {
    $c = New-WtPatch -Change @{ restart_needed = $true; logins_since_boot = 2; workers = 1 }
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'last-patch-reboot') -Text ("{0}`n" -f ($c.Now - 60)) -WrittenUnix ($c.Now - 60)
    $r = Invoke-WtPatch -Case $c
    Assert-WtNotRestarted -Case $c -Result $r
  }

  # --- The seam -----------------------------------------------------------------------------------------------------------
  Test-WtCase 'Seam: Under it the script starts no tools and calls no Windows Update: only the state folder changes' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    $before = (Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)
    [void](Invoke-WtPatch -Case $c)
    Assert-WtEqual ((Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)) $before 'The home''s .agentspace and .ssh (outside the case folder)'
  }
  $patchNames = @('AGENTSPACE_PATCH_STATE', 'AGENTSPACE_PATCH_FACTS')
  foreach ($only in @(0, 1)) {
    $onlyCopy = $only
    Test-WtCase ("Seam: Only {0} set is refused, nothing done" -f $patchNames[$only]) {
      $c = New-WtPatch -Change @{ restart_needed = $true }
      $all = @{ AGENTSPACE_PATCH_STATE = $c.State; AGENTSPACE_PATCH_FACTS = $c.FactsPath }
      $seam = @{ AGENTSPACE_PATCH_STATE = $null; AGENTSPACE_PATCH_FACTS = $null }
      $seam[$patchNames[$onlyCopy]] = $all[$patchNames[$onlyCopy]]
      $before = Get-WtSnapshot -Path $c.Dir
      $r = Invoke-WtPatch -Case $c -Seam $seam
      Assert-WtSeamRefusal -Result $r -ScriptName $scriptName -Variables $patchNames
      Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder (state, facts)'
    }
  }
  Test-WtCase 'Seam: A facts file that is missing is refused, nothing done' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Remove-WtFile -TestHome $testHome -Path $c.FactsPath
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtPatch -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains("$scriptName`: ") -and $r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: A facts file that is not valid JSON is refused, nothing done' {
    $c = New-WtPatch -Change @{ restart_needed = $true }
    Write-WtText -TestHome $testHome -Path $c.FactsPath -Text 'result = ok'
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtPatch -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

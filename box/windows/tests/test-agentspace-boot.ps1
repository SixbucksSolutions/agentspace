#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests box/windows/agentspace-boot.ps1 (E0.12-T03-S07, #0260; D96; architect's pin P17), written from
# docs/operator.md, "The boot task" and "The tests' seams", and not from the script: the boot task
# removes exactly the files operator.md lists (lease-horizon, last-active and busy from the idle
# state folder; patched and patch-failed from the patch state folder), keeps last-patch-reboot, writes
# boot.log, and in place of growing C: and of starting the patch task writes `partition-grown` and
# `patch-started` in the patch state folder and exits 0.
#
# Seams (operator.md, The tests' seams): AGENTSPACE_IDLE_STATE and AGENTSPACE_PATCH_STATE, set
# together. Under the seam the script touches nothing outside those two folders. The script's folder is
# -ScriptDir (default: box/windows beside this folder).
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'agentspace-boot'
$scriptPath = Join-Path (Resolve-WtScriptDir $ScriptDir) ($scriptName + '.ps1')
$testHome = New-WtHome

$idleFiles = @('lease-horizon', 'last-active', 'busy')
$patchFiles = @('patched', 'patch-failed')

# A case: an idle state folder and a patch state folder, each with the files listed seeded (when
# -Seed) and, beside them, files the boot task MUST leave.
function New-WtBoot {
  param([switch]$Seed)
  $dir = New-WtCase -TestHome $testHome
  $c = [pscustomobject]@{
    Dir = $dir
    Idle = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'idle-state')
    Patch = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'patch-state')
    Outside = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'outside')
  }
  if ($Seed) {
    $t = (Get-WtUnixNow) - 600
    foreach ($n in $idleFiles) { Write-WtText -TestHome $testHome -Path (Join-Path $c.Idle $n) -Text ("{0}`n" -f $t) }
    foreach ($n in $patchFiles) { Write-WtText -TestHome $testHome -Path (Join-Path $c.Patch $n) -Text ("{0}`n" -f $t) }
  }
  return $c
}

function Invoke-WtBoot {
  param([Parameter(Mandatory)]$Case, [hashtable]$Seam = $null)
  if ($null -eq $Seam) { $Seam = @{ AGENTSPACE_IDLE_STATE = $Case.Idle; AGENTSPACE_PATCH_STATE = $Case.Patch } }
  return Invoke-WtScript -TestHome $testHome -ScriptPath $scriptPath -Seam $Seam
}

try {
  Assert-WtScriptExists -Path $scriptPath
  $realBefore = Get-WtRealMark

  Test-WtCase 'Removal: The three idle files are removed from the idle state folder' {
    $c = New-WtBoot -Seed
    $r = Invoke-WtBoot -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    foreach ($n in $idleFiles) { Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $c.Idle $n))) ("{0} MUST be removed from the idle state folder. {1}" -f $n, (Format-WtRun $r)) }
  }
  Test-WtCase 'Removal: The two markers are removed from the patch state folder' {
    $c = New-WtBoot -Seed
    $r = Invoke-WtBoot -Case $c
    foreach ($n in $patchFiles) { Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $c.Patch $n))) ("{0} MUST be removed from the patch state folder. {1}" -f $n, (Format-WtRun $r)) }
  }
  Test-WtCase 'Removal: last-patch-reboot is kept, byte for byte, with its last write time' {
    $c = New-WtBoot -Seed
    $p = Join-Path $c.Patch 'last-patch-reboot'
    $t = (Get-WtUnixNow) - 86400
    Write-WtText -TestHome $testHome -Path $p -Text ("{0}`n" -f $t) -WrittenUnix $t
    $bytes = Read-WtBytes -Path $p
    $mtime = (Get-Item -LiteralPath $p).LastWriteTimeUtc
    [void](Invoke-WtBoot -Case $c)
    Assert-Wt (Test-Path -LiteralPath $p -PathType Leaf) 'last-patch-reboot MUST be kept'
    Assert-WtBytes -Actual (Read-WtBytes -Path $p) -Expected $bytes -What 'last-patch-reboot'
    Assert-Wt ((Get-Item -LiteralPath $p).LastWriteTimeUtc -eq $mtime) 'last-patch-reboot MUST NOT be rewritten'
  }
  Test-WtCase 'Removal: Every other file in the two folders is left byte for byte' {
    $c = New-WtBoot -Seed
    $others = @{
      (Join-Path $c.Idle 'idle.log') = "a line`n"
      (Join-Path $c.Idle 'poweroff') = "1764000000`n"
      (Join-Path $c.Idle 'lease-horizon.old') = "1764000000`n"
      (Join-Path $c.Idle 'notes.txt') = "keep me`n"
      (Join-Path $c.Patch 'patch.log') = "a patch line`n"
      (Join-Path $c.Patch 'patched.keep') = "x`n"
      (Join-Path $c.Patch 'last-patch-reboot') = "1764000000`n"
      (Join-Path $c.Patch 'restart') = "1764000000`n"
    }
    foreach ($p in $others.Keys) { Write-WtText -TestHome $testHome -Path $p -Text $others[$p] }
    [void](Invoke-WtBoot -Case $c)
    foreach ($p in $others.Keys) {
      Assert-Wt (Test-Path -LiteralPath $p -PathType Leaf) ("{0} MUST be kept" -f (Split-Path -Leaf $p))
      Assert-WtBytes -Actual (Read-WtBytes -Path $p) -Expected ($script:WtUtf8.GetBytes($others[$p])) -What (Split-Path -Leaf $p)
    }
  }
  Test-WtCase 'Removal: A file whose name is not on the list stays in each folder' {
    $c = New-WtBoot -Seed
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Idle 'keepme') -Text "1`n"
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Patch 'keepme') -Text "1`n"
    [void](Invoke-WtBoot -Case $c)
    Assert-Wt ((Get-WtNames -Path $c.Idle) -contains 'keepme') 'An unlisted file in the idle state folder MUST stay'
    Assert-Wt ((Get-WtNames -Path $c.Patch) -contains 'keepme') 'An unlisted file in the patch state folder MUST stay'
  }
  Test-WtCase 'Removal: A listed name in the other folder is not removed (each folder has its own list)' {
    $c = New-WtBoot -Seed
    # patched belongs to the patch folder's list, lease-horizon to the idle folder's: swap them.
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Idle 'patched') -Text "idle-side`n"
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Patch 'lease-horizon') -Text "patch-side`n"
    [void](Invoke-WtBoot -Case $c)
    Assert-Wt (Test-Path -LiteralPath (Join-Path $c.Idle 'patched')) 'patched in the idle state folder is not on that folder''s list and MUST stay'
    Assert-Wt (Test-Path -LiteralPath (Join-Path $c.Patch 'lease-horizon')) 'lease-horizon in the patch state folder is not on that folder''s list and MUST stay'
  }
  Test-WtCase 'Removal: A file that is not there is no error' {
    $c = New-WtBoot
    $r = Invoke-WtBoot -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
  }
  Test-WtCase 'Removal: Only some of the files being there is no error, and those go' {
    $c = New-WtBoot
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Idle 'busy') -Text "5`n"
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Patch 'patch-failed') -Text "5`n"
    $r = Invoke-WtBoot -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
    Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $c.Idle 'busy'))) 'busy MUST be removed'
    Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $c.Patch 'patch-failed'))) 'patch-failed MUST be removed'
  }

  Test-WtCase 'Stand-ins: In place of growing C: it writes partition-grown in the patch state folder, one Unix time and LF' {
    $c = New-WtBoot -Seed
    $r = Invoke-WtBoot -Case $c
    $t = Read-WtUnixLine -Path (Join-Path $c.Patch 'partition-grown')
    Assert-Wt ($null -ne $t) ("partition-grown MUST be written in the patch state folder. {0}" -f (Format-WtRun $r))
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'partition-grown MUST hold the time as Unix seconds'
  }
  Test-WtCase 'Stand-ins: In place of starting the patch task it writes patch-started in the patch state folder, one Unix time and LF' {
    $c = New-WtBoot -Seed
    $r = Invoke-WtBoot -Case $c
    $t = Read-WtUnixLine -Path (Join-Path $c.Patch 'patch-started')
    Assert-Wt ($null -ne $t) ("patch-started MUST be written in the patch state folder. {0}" -f (Format-WtRun $r))
    Assert-Wt ([math]::Abs($t - (Get-WtUnixNow)) -lt 300) 'patch-started MUST hold the time as Unix seconds'
  }
  Test-WtCase 'Stand-ins: Neither stand-in mark is written in the idle state folder' {
    $c = New-WtBoot -Seed
    [void](Invoke-WtBoot -Case $c)
    foreach ($n in @('partition-grown', 'patch-started')) { Assert-Wt (-not (Test-Path -LiteralPath (Join-Path $c.Idle $n))) ("{0} MUST be written in the patch state folder alone" -f $n) }
  }
  Test-WtCase 'Stand-ins: The patch task is started after the old markers are removed, so a marker it leaves is its own' {
    # patch-started and the removal both happen: no old marker is left beside the new mark.
    $c = New-WtBoot -Seed
    [void](Invoke-WtBoot -Case $c)
    $names = Get-WtNames -Path $c.Patch
    Assert-Wt ($names -contains 'patch-started') 'patch-started MUST be written'
    Assert-Wt (-not ($names -contains 'patched') -and -not ($names -contains 'patch-failed')) 'The old markers MUST be gone'
  }
  Test-WtCase 'Run: It exits 0' {
    $c = New-WtBoot -Seed
    $r = Invoke-WtBoot -Case $c
    Assert-WtEqual $r.ExitCode 0 'Exit code'
  }
  Test-WtCase 'Log: boot.log is written in the patch state folder, with a line holding the date in UTC' {
    $c = New-WtBoot -Seed
    [void](Invoke-WtBoot -Case $c)
    $log = Join-Path $c.Patch 'boot.log'
    Assert-Wt (Test-Path -LiteralPath $log -PathType Leaf) 'boot.log MUST be written in the patch state folder'
    $first = (Get-WtLines -Text (Read-WtText -Path $log))[0]
    Assert-Wt ($first -match '\d{4}-\d{2}-\d{2}') ("A log line MUST hold the date in UTC: {0}" -f $first)
  }
  Test-WtCase 'Log: A run again appends to boot.log, a line at a time' {
    $c = New-WtBoot -Seed
    [void](Invoke-WtBoot -Case $c)
    $log = Join-Path $c.Patch 'boot.log'
    $first = Read-WtText -Path $log
    [void](Invoke-WtBoot -Case $c)
    $second = Read-WtText -Path $log
    Assert-Wt ($second.StartsWith($first) -and $second.Length -gt $first.Length) 'boot.log MUST be appended to, never rewritten'
  }
  Test-WtCase 'Log: A log that grew past 1 MiB is renamed over boot.log.1 before the next line' {
    $c = New-WtBoot -Seed
    $big = [byte[]]::new(1100000)
    [Array]::Fill($big, [byte]98)
    Write-WtBytes -TestHome $testHome -Path (Join-Path $c.Patch 'boot.log') -Bytes $big
    [void](Invoke-WtBoot -Case $c)
    $rolled = Join-Path $c.Patch 'boot.log.1'
    Assert-Wt (Test-Path -LiteralPath $rolled -PathType Leaf) 'boot.log.1 MUST hold the old log'
    Assert-WtEqual ((Get-Item -LiteralPath $rolled).Length) 1100000 'boot.log.1 size'
  }

  Test-WtCase 'Seam: Under it the script touches nothing outside the two folders' {
    $c = New-WtBoot -Seed
    Write-WtText -TestHome $testHome -Path (Join-Path $c.Outside 'canary.txt') -Text "untouched`n"
    $beforeOutside = Get-WtSnapshot -Path $c.Outside
    $beforeProfile = (Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)
    [void](Invoke-WtBoot -Case $c)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Outside) $beforeOutside 'A folder beside the two state folders'
    Assert-WtEqual ((Get-WtSnapshot -Path (Split-Path -Parent $testHome.StateDir)) + (Get-WtSnapshot -Path $testHome.SshDir)) $beforeProfile 'The home''s .agentspace and .ssh'
    Assert-WtEqual (@(Get-WtNames -Path $c.Dir).Count) 3 'Entries in the case folder (idle-state, patch-state, outside)'
  }
  $bootNames = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_PATCH_STATE')
  foreach ($only in @(0, 1)) {
    $onlyCopy = $only
    Test-WtCase ("Seam: Only {0} set is refused, nothing done" -f $bootNames[$only]) {
      $c = New-WtBoot -Seed
      $all = @{ AGENTSPACE_IDLE_STATE = $c.Idle; AGENTSPACE_PATCH_STATE = $c.Patch }
      $seam = @{ AGENTSPACE_IDLE_STATE = $null; AGENTSPACE_PATCH_STATE = $null }
      $seam[$bootNames[$onlyCopy]] = $all[$bootNames[$onlyCopy]]
      $before = Get-WtSnapshot -Path $c.Dir
      $r = Invoke-WtBoot -Case $c -Seam $seam
      Assert-WtSeamRefusal -Result $r -ScriptName $scriptName -Variables $bootNames
      Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder (both state folders)'
    }
  }
  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

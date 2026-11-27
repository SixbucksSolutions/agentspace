#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests box/windows/agentspace-patch-check.ps1 (E0.12-T03-S07, #0260; N6; architect's pin P17), written
# from docs/operator.md, "The patch run and its restart rule" (the patch check) and "The tests' seams",
# and not from the script. The check answers one word, in this order:
#   patched  when `patched` exists and was written this boot;
#   failed   when the patch task is neither running nor queued, and either `patch-failed` exists and was
#            written this boot, or the task's last run started this boot and its last result is not 0;
#   pending  when the patch task is registered and enabled;
#   unknown  otherwise.
#
# Seams (operator.md, The tests' seams): AGENTSPACE_PATCH_STATE, the folder holding the markers, and
# AGENTSPACE_PATCH_FACTS, a JSON file with boot (this boot's start, Unix seconds), task_state (running,
# queued, ready, disabled or missing), task_last_run (Unix seconds or null) and task_last_result (a
# number or null). A marker counts as written this boot when its last write is at or after `boot`. The
# marker times here are set from `boot`, so no case depends on the clock. The script's folder is
# -ScriptDir (default: box/windows beside this folder).
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$scriptName = 'agentspace-patch-check'
$scriptPath = Join-Path (Resolve-WtScriptDir $ScriptDir) ($scriptName + '.ps1')
$testHome = New-WtHome
$bootUnix = [long]1760000000

# A case: a state folder, the facts for a task in $TaskState, and the markers asked for. A marker is
# 'fresh' (written 5 minutes after boot), 'at' (exactly at boot), 'before' (one second before boot) or
# absent.
function New-WtCheck {
  param(
    [Parameter(Mandatory)][string]$TaskState,
    [string]$Patched = '', [string]$Failed = '',
    $LastRun = $null, $LastResult = $null
  )
  $dir = New-WtCase -TestHome $testHome
  $c = [pscustomobject]@{
    Dir = $dir
    State = New-WtFolder -TestHome $testHome -Path (Join-Path $dir 'state')
    FactsPath = Join-Path $dir 'facts.json'
  }
  $times = @{ fresh = $bootUnix + 300; at = $bootUnix; before = $bootUnix - 1 }
  foreach ($pair in @(@('patched', $Patched), @('patch-failed', $Failed))) {
    if ($pair[1] -ne '') { Write-WtText -TestHome $testHome -Path (Join-Path $c.State $pair[0]) -Text ("{0}`n" -f $times[$pair[1]]) -WrittenUnix $times[$pair[1]] }
  }
  $facts = [ordered]@{ boot = $bootUnix; task_state = $TaskState; task_last_run = $LastRun; task_last_result = $LastResult }
  Write-WtJson -TestHome $testHome -Path $c.FactsPath -Object $facts
  return $c
}

function Invoke-WtCheck {
  param([Parameter(Mandatory)]$Case, [hashtable]$Seam = $null)
  if ($null -eq $Seam) { $Seam = @{ AGENTSPACE_PATCH_STATE = $Case.State; AGENTSPACE_PATCH_FACTS = $Case.FactsPath } }
  return Invoke-WtScript -TestHome $testHome -ScriptPath $scriptPath -Seam $Seam
}

# The answer is one word on stdout and nothing else there.
function Assert-WtAnswer {
  param([Parameter(Mandatory)]$Case, [Parameter(Mandatory)][string]$Word)
  $r = Invoke-WtCheck -Case $Case
  $lines = @(Get-WtLines -Text $r.Stdout)
  Assert-Wt ($lines.Count -eq 1 -and $lines[0] -ceq $Word) ("The answer MUST be the one word '{0}'. {1}" -f $Word, (Format-WtRun $r))
}

try {
  Assert-WtScriptExists -Path $scriptPath
  $realBefore = Get-WtRealMark

  # --- patched ------------------------------------------------------------------------------------------------
  Test-WtCase 'Patched: patched written this boot, the task ready, answers patched' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Patched fresh) -Word 'patched'
  }
  Test-WtCase 'Patched: patched written exactly at this boot''s start counts' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Patched at) -Word 'patched'
  }
  Test-WtCase 'Patched: patched written one second before this boot does not count: the task ready answers pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Patched before) -Word 'pending'
  }
  Test-WtCase 'Patched: patched from before this boot with the task missing answers unknown' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState missing -Patched before) -Word 'unknown'
  }
  Test-WtCase 'Patched: patched written this boot is first, whatever the task is doing (running)' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState running -Patched fresh) -Word 'patched'
  }
  Test-WtCase 'Patched: patched written this boot is first, whatever the task is doing (missing)' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState missing -Patched fresh) -Word 'patched'
  }
  Test-WtCase 'Patched: patched written this boot is first, ahead of a patch-failed written this boot' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Patched fresh -Failed fresh) -Word 'patched'
  }
  Test-WtCase 'Patched: patched written this boot is first, ahead of a failed last run' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Patched fresh -LastRun ($bootUnix + 60) -LastResult 1) -Word 'patched'
  }

  # --- failed ------------------------------------------------------------------------------------------------------
  Test-WtCase 'Failed: patch-failed written this boot, the task ready, answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Failed fresh) -Word 'failed'
  }
  Test-WtCase 'Failed: patch-failed written exactly at this boot''s start counts' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Failed at) -Word 'failed'
  }
  Test-WtCase 'Failed: patch-failed written one second before this boot does not count: the task ready answers pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Failed before) -Word 'pending'
  }
  Test-WtCase 'Failed: patch-failed this boot with the task disabled (neither running nor queued) answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState disabled -Failed fresh) -Word 'failed'
  }
  Test-WtCase 'Failed: patch-failed this boot with the task missing (neither running nor queued) answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState missing -Failed fresh) -Word 'failed'
  }
  Test-WtCase 'Failed: patch-failed this boot with the task running is not failed yet: pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState running -Failed fresh) -Word 'pending'
  }
  Test-WtCase 'Failed: patch-failed this boot with the task queued is not failed yet: pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState queued -Failed fresh) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started this boot with a result that is not 0 answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun ($bootUnix + 120) -LastResult 1) -Word 'failed'
  }
  Test-WtCase 'Failed: A last run started this boot with a large result (a Task Scheduler code) answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun ($bootUnix + 120) -LastResult 267009) -Word 'failed'
  }
  Test-WtCase 'Failed: A last run that started exactly at this boot''s start with a result not 0 answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun $bootUnix -LastResult 2) -Word 'failed'
  }
  Test-WtCase 'Failed: A last run started this boot with result 0 and no marker is not failed: pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun ($bootUnix + 120) -LastResult 0) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started before this boot with a result not 0 is not failed: pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun ($bootUnix - 600) -LastResult 1) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started one second before this boot with a result not 0 is not failed: pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -LastRun ($bootUnix - 1) -LastResult 1) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started this boot, result not 0, the task running (a run again) is pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState running -LastRun ($bootUnix + 120) -LastResult 1) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started this boot, result not 0, the task queued is pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState queued -LastRun ($bootUnix + 120) -LastResult 1) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run started this boot, result not 0, the task disabled answers failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState disabled -LastRun ($bootUnix + 120) -LastResult 1) -Word 'failed'
  }
  Test-WtCase 'Failed: A task never run (no last run, no result) with no marker is pending, not failed' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready) -Word 'pending'
  }
  Test-WtCase 'Failed: A last run this boot with result 0 beside patch-failed this boot answers failed (the marker rule alone)' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready -Failed fresh -LastRun ($bootUnix + 120) -LastResult 0) -Word 'failed'
  }

  # --- pending -------------------------------------------------------------------------------------------------------
  Test-WtCase 'Pending: A task registered and ready with no marker answers pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState ready) -Word 'pending'
  }
  Test-WtCase 'Pending: A task running with no marker answers pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState running) -Word 'pending'
  }
  Test-WtCase 'Pending: A task queued with no marker answers pending' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState queued) -Word 'pending'
  }

  # --- unknown -------------------------------------------------------------------------------------------------------
  Test-WtCase 'Unknown: A task not registered, no marker, answers unknown' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState missing) -Word 'unknown'
  }
  Test-WtCase 'Unknown: A task registered and disabled, no marker, answers unknown' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState disabled) -Word 'unknown'
  }
  Test-WtCase 'Unknown: A disabled task with markers from before this boot answers unknown' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState disabled -Patched before -Failed before) -Word 'unknown'
  }
  Test-WtCase 'Unknown: A missing task whose last run failed before this boot answers unknown' {
    Assert-WtAnswer -Case (New-WtCheck -TaskState missing -LastRun ($bootUnix - 600) -LastResult 1) -Word 'unknown'
  }

  # --- The form of the answer ----------------------------------------------------------------------------------------
  Test-WtCase 'Form: The word is on stdout and stderr is empty' {
    $c = New-WtCheck -TaskState ready -Patched fresh
    $r = Invoke-WtCheck -Case $c
    Assert-WtEqual $r.Stderr.Trim() '' 'stderr'
  }
  Test-WtCase 'Form: The check changes nothing: the state folder is the same after the run' {
    $c = New-WtCheck -TaskState ready -Patched fresh -Failed fresh
    $before = Get-WtSnapshot -Path $c.Dir
    $mt = (Get-Item -LiteralPath (Join-Path $c.State 'patched')).LastWriteTimeUtc
    [void](Invoke-WtCheck -Case $c)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
    Assert-Wt ((Get-Item -LiteralPath (Join-Path $c.State 'patched')).LastWriteTimeUtc -eq $mt) 'A marker MUST NOT be rewritten'
  }
  Test-WtCase 'Form: The marker''s content does not matter, only its last write time' {
    $c = New-WtCheck -TaskState ready
    Write-WtText -TestHome $testHome -Path (Join-Path $c.State 'patched') -Text 'garbage' -WrittenUnix ($bootUnix + 10)
    Assert-WtAnswer -Case $c -Word 'patched'
  }

  # --- The seam --------------------------------------------------------------------------------------------------------
  $checkNames = @('AGENTSPACE_PATCH_STATE', 'AGENTSPACE_PATCH_FACTS')
  foreach ($only in @(0, 1)) {
    $onlyCopy = $only
    Test-WtCase ("Seam: Only {0} set is refused, nothing done" -f $checkNames[$only]) {
      $c = New-WtCheck -TaskState ready -Patched fresh
      $all = @{ AGENTSPACE_PATCH_STATE = $c.State; AGENTSPACE_PATCH_FACTS = $c.FactsPath }
      $seam = @{ AGENTSPACE_PATCH_STATE = $null; AGENTSPACE_PATCH_FACTS = $null }
      $seam[$checkNames[$onlyCopy]] = $all[$checkNames[$onlyCopy]]
      $before = Get-WtSnapshot -Path $c.Dir
      $r = Invoke-WtCheck -Case $c -Seam $seam
      Assert-WtSeamRefusal -Result $r -ScriptName $scriptName -Variables $checkNames
      Assert-Wt (@(Get-WtLines -Text $r.Stdout).Count -eq 0) ("A refused seam MUST print no answer: {0}" -f $r.Stdout)
      Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
    }
  }
  Test-WtCase 'Seam: A facts file that is missing is refused, nothing done' {
    $c = New-WtCheck -TaskState ready -Patched fresh
    Remove-WtFile -TestHome $testHome -Path $c.FactsPath
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtCheck -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains("$scriptName`: ") -and $r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-Wt (@(Get-WtLines -Text $r.Stdout).Count -eq 0) ("No answer MUST be printed: {0}" -f $r.Stdout)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: A facts file that is not valid JSON is refused, nothing done' {
    $c = New-WtCheck -TaskState ready -Patched fresh
    Write-WtText -TestHome $testHome -Path $c.FactsPath -Text '[1, 2'
    $before = Get-WtSnapshot -Path $c.Dir
    $r = Invoke-WtCheck -Case $c
    Assert-WtEqual $r.ExitCode 1 'Exit code'
    Assert-Wt ($r.Stderr.Contains(" cannot be read as the test's facts; nothing was done")) ("stderr MUST say the facts cannot be read: {0}" -f $r.Stderr)
    Assert-Wt (@(Get-WtLines -Text $r.Stdout).Count -eq 0) ("No answer MUST be printed: {0}" -f $r.Stdout)
    Assert-WtEqual (Get-WtSnapshot -Path $c.Dir) $before 'The case folder'
  }
  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

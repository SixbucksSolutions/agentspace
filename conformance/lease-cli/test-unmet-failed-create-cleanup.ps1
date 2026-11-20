#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement, a MUST (architect's ruling, v0.04-T05, carried by E0.04-T05-S05, #0092; found while
# marking driver.md's Q8): a failed `create` MUST remove every file and folder it made, and when a
# removal itself fails it MUST name each leftover in its `internal` message, in its home-relative form
# (never an absolute path), MUST leave no lease record (so none that is `ready`), and MUST change
# nothing else.
#   - docs/lease-api.md, "Concurrency and atomicity" (the failed `create` bullet) and "Exit codes and
#     errors" (the `internal` message rule).
#   - docs/state.md, "Creation, in order" (what a failed `create` leaves).
# What the prototype does instead: its cleanup fails with a runtime message that holds the absolute
# path of the lease's folder, and names the leftover in no home-relative form.
#
# How a removal is made to fail: the throwaway home's `.agentspace\leases` folder gets a deny rule for
# the current user, inherited by what is made under it, that forbids deleting (Delete and Delete
# subfolders and files). Making a folder or a file under it is still allowed. The check first proves,
# with a probe folder, that the rule stops a delete, and reports SKIPPED if it does not. The create is
# made to fail after the credential exists by putting a FILE where the alias folder `agentspace.d` has
# to be (docs/state.md, "Creation, in order"), as test-lease-driver-outcomes.ps1 does. The rule is
# lifted again, in a `finally`, before the throwaway home is removed.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - Every write and delete it makes (the rule, the probe, the file in the way) goes through
#   Assert-ConformancePath first. It builds no path from the program's output: the leftovers are found
#   by listing the throwaway home's own `leases` folder.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-failed-create-cleanup.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$sec = 'lease-api.md#concurrency-and-atomicity'

# Sets or lifts the deny rule on the throwaway home's leases folder. icacls is a program outside
# agentspace. The rule is for the current user's SID, inherited by folders and files made under it.
function Set-CcLeasesDeny($Lab, [string]$Folder, [bool]$On) {
  $full = Assert-ConformancePath -ConformanceHome $Lab -Path $Folder
  $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $icacls = Join-Path $env:SystemRoot 'System32\icacls.exe'
  if ($On) { $out = & $icacls (Assert-ConformancePath -ConformanceHome $Lab -Path $full) /deny "*${sid}:(OI)(CI)(DE,DC)" 2>&1 } else { $out = & $icacls (Assert-ConformancePath -ConformanceHome $Lab -Path $full) /remove:d "*$sid" 2>&1 }
  if ($LASTEXITCODE -ne 0) { throw "icacls failed with exit code $LASTEXITCODE" }
}

# Every leftover under a folder: a file, or a folder that holds nothing. A folder with a file in it is
# not a leftover of its own, since the file's name says where it is.
function Get-CcLeftovers([string]$Folder) {
  if (-not (Test-Path -LiteralPath $Folder -PathType Container)) { return @() }
  @(Get-ChildItem -LiteralPath $Folder -Recurse -Force | Where-Object {
      -not $_.PSIsContainer -or -not (Get-ChildItem -LiteralPath $_.FullName -Force | Select-Object -First 1)
    } | ForEach-Object FullName)
}

# The home-relative form of a path under the home, as ~/.agentspace/leases/<id>/id_ed25519.
function ConvertTo-CcHomeRelative($Lab, [string]$Path) {
  $home1 = [IO.Path]::GetFullPath($Lab.UserProfile).TrimEnd('\')
  $full = [IO.Path]::GetFullPath($Path)
  '~/' + $full.Substring($home1.Length + 1).Replace('\', '/')
}

$lab = $null
$denied = $false
$leases = ''
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: A failed create whose cleanup fails names its leftovers" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    # Something else that exists, which a failed create MUST NOT change: an unsatisfiable lease's record.
    $seed = Invoke-CcLease $lab @('create', '--project', 'cleanup', '--size', 'l')
    $leases = Join-Path $lab.UserProfile '.agentspace\leases'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $leases) | Out-Null

    # The fault, and proof that it works: a probe folder under the rule cannot be deleted.
    $denied = $true
    Set-CcLeasesDeny $lab $leases $true
    $probe = Join-Path $leases 'zz-probe'
    New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $lab -Path $probe) | Out-Null
    $probeRemoved = $true
    try { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $probe) -Force -ErrorAction Stop } catch { $probeRemoved = $false }
    Set-CcLeasesDeny $lab $leases $false
    $denied = $false
    if (Test-Path -LiteralPath $probe) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $probe) -Force }
    if (-not (Test-CcSuccess $seed)) {
      Write-ConformanceResult -Name "$sec`: Setup, an unsatisfiable create records a lease for the failed create to leave alone" -Status FAIL -Reason "The create exited with code $($seed.ExitCode), or did not print a valid lease"
    } elseif ($probeRemoved) {
      Write-ConformanceResult -Name "$sec`: A failed create whose cleanup fails names its leftovers" -Status SKIPPED -Reason 'The deny rule did not stop a delete on this machine, so no removal could be made to fail'
    } else {
      $blocker = Join-Path $lab.SshDir 'agentspace.d'
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $lab -Path $blocker), "not a folder`n")
      $stateBefore = Get-CcStateCanonical $lab
      $sshBefore = (Get-CcFileNames $lab.SshDir) -join '|'

      $denied = $true
      Set-CcLeasesDeny $lab $leases $true
      $run = Invoke-CcLease $lab @('create', '--project', 'cleanup')
      $left = @(Get-CcLeftovers $leases)
      $sshAfter = Get-CcFileNames $lab.SshDir
      $stateAfter = Read-CcState $lab
      $blockerSame = (Get-Content -LiteralPath $blocker -Raw) -ceq "not a folder`n"
      Set-CcLeasesDeny $lab $leases $false
      $denied = $false

      $err = ConvertFrom-CcJson $run.Stderr
      $message = if ($err) { [string]$err['message'] } else { '' }

      Test-Cc "$sec`: A create whose cleanup fails ends with internal (exit 1), nothing on stdout and one valid error document" {
        if (Test-CcFailure $run 1 'internal') { $true } else { "Not the expected failure: exit $($run.ExitCode), $($run.StdoutBytes.Length) bytes on stdout" }
      }
      Test-Cc "$sec`: The failed create left something the rule kept it from removing, so there is a leftover to name" {
        if ($left.Count -gt 0) { $true } else { 'Nothing was left under the leases folder: the fault did not bite, so the next result proves nothing' }
      }
      Test-Cc "$sec`: The message names each leftover in its home-relative form (~/.agentspace/leases/<id>/...)" {
        if (-not $message) { return 'There was no error message.' }
        $normal = $message.Replace('\', '/')
        $missing = 0
        foreach ($l in $left) { if (-not $normal.Contains((ConvertTo-CcHomeRelative $lab $l))) { $missing++ } }
        if ($missing -gt 0) { return "$missing of $($left.Count) leftovers are not named in their home-relative form." }
        $true
      }
      Test-Cc "$sec`: The message holds no absolute path and no home folder (lease-api.md#exit-codes-and-errors)" {
        if (-not $message) { return 'There was no error message.' }
        if ($message -match '(?i)[a-z]:[\\/]' -or $message.Contains($lab.Root) -or $message.Contains($lab.UserProfile)) { return 'The message holds an absolute path or the home folder.' }
        $true
      }
      Test-Cc "$sec`: The failed create leaves no lease record, so none is ready" {
        if ($null -eq $stateAfter) { return $true }
        $seedId = [string](ConvertFrom-CcJson $seed.Stdout)['id']
        $others = @($stateAfter['leases'] | Where-Object { $_['id'] -cne $seedId }).Count
        $ready = @($stateAfter['leases'] | Where-Object { $_['state'] -ceq 'ready' }).Count
        if ($others -eq 0 -and $ready -eq 0) { $true } else { "$others record(s) besides the seed's, and $ready ready." }
      }
      Test-Cc "$sec`: The failed create changes nothing else: the seed lease's record, the file in the way and the ssh folder's entries" {
        if ($null -eq $stateAfter) { return 'The state file is gone.' }
        $seedId = [string](ConvertFrom-CcJson $seed.Stdout)['id']
        $seedBefore = @((ConvertFrom-CcJson ($stateBefore))['leases'] | Where-Object { $_['id'] -ceq $seedId })
        $seedAfter = @($stateAfter['leases'] | Where-Object { $_['id'] -ceq $seedId })
        if ($seedBefore.Count -ne 1 -or $seedAfter.Count -ne 1) { return 'The seed lease record is not exactly once in the state file.' }
        if ((ConvertTo-CcCanonical $seedBefore[0]) -cne (ConvertTo-CcCanonical $seedAfter[0])) { return 'The seed lease record changed.' }
        if (-not $blockerSame) { return 'The file in the way was changed.' }
        # The ssh folder MAY gain the Include line's config and a dated backup of it, and nothing else.
        $extra = @($sshAfter | Where-Object { $_ -cne 'agentspace.d' -and $_ -notlike 'config*' })
        if ($extra.Count -gt 0) { return "$($extra.Count) unexpected entries are in the ssh folder." }
        if ($sshAfter -cnotcontains 'agentspace.d') { return 'The file in the way is gone.' }
        $true
      }
      Test-Cc "$sec`: A later create is not blocked by what the failed one left (state.md#creation-in-order)" {
        Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $blocker) -Force
        $later = New-CcReadyLease $lab 'cleanup'
        if ($later.Ok) { $true } else { "The create exited with code $($later.Run.ExitCode), or did not print a ready lease" }
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-failed-create-cleanup.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  # Lift the rule first, or the throwaway home cannot be removed.
  if ($denied -and $lab -and $leases) { try { Set-CcLeasesDeny $lab $leases $false } catch { } }
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
if ($global:ConformanceCounts['FAIL'] -gt 0) {
  Write-Host 'NOTE This file is expected to fail against the prototype (the failed-create cleanup requirement); a rewrite MUST pass it.'
}
Exit-ConformanceRun

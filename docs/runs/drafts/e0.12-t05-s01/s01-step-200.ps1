# Step 200 of the v0.12-T05 linux/amd64 live check: scripts\live\mask-proof.ps1 passes. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. See s01-common.ps1 for the exit codes and the dry-run variables.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '200' 'scripts\live\mask-proof.ps1 passes'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $proofDir = Join-Path (Get-S01StateDir) 'mask-proof'
    Write-S01 'Command: pwsh -NoProfile -File scripts\live\mask-proof.ps1 -ProofDir <a folder under target>'
    $run = Invoke-S01Tool -Tool 'maskproof' -Arguments @('-ProofDir', $proofDir) -TimeoutSec 300
    if ($run.StartFailed -or $run.TimedOut) { $p.Add('the mask proof could not be run to its end'); return (Complete-S01 $p) }
    Write-S01 "Exit: $($run.ExitCode)"
    $lines = @(($run.Stdout + "`n" + $run.Stderr) -split "\r?\n" | Where-Object { $_ -ne '' })
    foreach ($l in $lines) { Write-S01 ('  ' + $l) }
    if ($run.ExitCode -ne 0) { $p.Add("mask-proof.ps1 exited $($run.ExitCode), expected 0") }
    $summary = @($lines | Where-Object { $_ -cmatch '^Mask proof: \d+ passed, 0 failed\.$' })
    Write-S01 "Summary lines of 'N passed, 0 failed': $($summary.Count)"
    if ($summary.Count -ne 1) { $p.Add('the line "Mask proof: <n> passed, 0 failed." is not there exactly once') }
    $failed = @($lines | Where-Object { $_ -cmatch '^FAILED:' })
    if ($failed.Count -gt 0) { $p.Add("$($failed.Count) FAILED: line(s)") }
    $true4 = @($lines | Where-Object { $_ -cmatch ' removed: True$' })
    $falseLines = @($lines | Where-Object { $_ -cmatch ': False$' })
    Write-S01 "Environment lines reading True: $($true4.Count); reading False: $($falseLines.Count)"
    if ($true4.Count -ne 4 -or $falseLines.Count -gt 0) { $p.Add('the four environment lines (computer name, user name, profile, short profile) do not all read True') }
    return (Complete-S01 $p)
  })

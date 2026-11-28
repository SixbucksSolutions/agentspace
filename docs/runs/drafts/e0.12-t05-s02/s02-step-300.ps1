# Step 300 of E0.12-T05-S02: scripts\live\mask-proof.ps1 passes. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing. The proof is a pure script of the checkout; S02_MASKPROOF names a
# planted one in a dry run.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '300' 'the live kit''s mask proof passes')) { exit 5 }
exit (Invoke-S02Main {
    $proof = if ($env:S02_MASKPROOF) { [IO.Path]::GetFullPath($env:S02_MASKPROOF) } else { Join-Path $script:S02Repo 'scripts\live\mask-proof.ps1' }
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = [Environment]::ProcessPath
    foreach ($a in @('-NoProfile', '-NonInteractive', '-File', $proof)) { $psi.ArgumentList.Add($a) }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [Diagnostics.Process]::Start($psi)
    $out = [IO.MemoryStream]::new(); $err = [IO.MemoryStream]::new()
    $tasks = @($p.StandardOutput.BaseStream.CopyToAsync($out), $p.StandardError.BaseStream.CopyToAsync($err))
    $p.StandardInput.Close()
    $timedOut = $false
    if (-not $p.WaitForExit(600000)) { $timedOut = $true; try { $p.Kill($true) } catch { }; [void]$p.WaitForExit(10000) }
    foreach ($t in $tasks) { [void]$t.Wait(10000) }
    $text = $script:S02Utf8.GetString($out.ToArray())
    $lines = @($text -split "\r?\n" | Where-Object { $_ -ne '' })
    $tail = @($lines | Select-Object -Last 6)
    foreach ($l in $tail) { Write-S02Line ("proof: " + $l) }
    $code = if ($timedOut) { -1 } else { $p.ExitCode }
    Write-S02Line "mask-proof exit: $code"
    Assert-S02 ($code -eq 0) 'The mask proof exited 0.' "The mask proof exited $code; no live command MUST run before it passes."
    $m = [regex]::Match($text, 'Mask proof: (\d+) passed, (\d+) failed\.')
    Assert-S02 ($m.Success -and [int]$m.Groups[2].Value -eq 0 -and [int]$m.Groups[1].Value -gt 0) 'The proof reports passed cases and 0 failed.' 'The proof does not report "N passed, 0 failed".'
    foreach ($label in 'COMPUTERNAME removed', 'USERNAME removed', 'USERPROFILE removed', 'USERPROFILE short form removed') {
      Assert-S02 ($text.Contains($label + ': True')) "$label`: True" "$label is not True."
    }
  })

# Step 313 of E0.12-T05-S02: the proofs before anything is handed to Terry (architect.md, "Learned in v0.12-T01"): `status -Host agentspace
# -Platform windows/amd64` (signs the operator's one-hour certificate, writes the operator alias); then, for the operator alias and for the
# lease's alias, `ssh -G <alias>` filtered to print whether a ProxyCommand line exists (never the user line, never the line itself), and one
# real `ssh <alias> true` with exit 0. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '313' 'proofs before Terry: status -Host (signs), ssh -G and one real ssh true, for both aliases')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $leaseAlias = Read-S02State 'lease-w.alias'
    $r = Invoke-S02Tool -Tool 'operator' -Arguments @('status', '-Host', 'agentspace', '-Platform', 'windows/amd64') -TimeoutSeconds 240
    Write-S02Line "status -Host agentspace -Platform windows/amd64 exit: $($r.ExitCode)"
    $text = $r.Stdout + "`n" + $r.Stderr
    foreach ($line in ($text -split "\r?\n")) { if ($line -ne '') { Write-S02Line ('  ' + $line) } }
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'status -Host exited 0 (it signed the operator certificate and wrote the alias).' "status -Host exited $($r.ExitCode); the operator's certificate was not signed."
    $first = @($text -split "\r?\n" | Where-Object { $_ -ne '' } | Select-Object -First 1)
    Assert-S02 ($first.Count -eq 1 -and $first[0] -ceq 'Platform  windows/amd64') 'The report starts with the Platform line, windows/amd64.' "The report's first line is not 'Platform  windows/amd64'."
    Assert-S02 ($text -cmatch '(?m)^State\s{5}running\s*$') 'The host is running.' 'The report has no "State     running" line.'
    Assert-S02 ($text -cmatch '(?m)^SSM\s{7}online\s*$') 'Session Manager is online.' 'The report has no "SSM       online" line.'

    foreach ($kind in 'operator', 'lease') {
      $alias = if ($kind -ceq 'operator') { $script:S02OperatorAlias } else { $leaseAlias }
      $label = if ($kind -ceq 'operator') { '<operator alias>' } else { '<alias>' }
      $g = Invoke-S02Tool -Tool 'ssh' -Arguments @('-G', $alias) -TimeoutSeconds 60
      $has = $false
      foreach ($line in ($g.Stdout -split "\r?\n")) { if ($line -imatch '^proxycommand\s+\S') { $has = $true } }
      Write-S02Line "$label ssh -G exit: $($g.ExitCode); a ProxyCommand line exists: $has"
      Assert-S02 ($g.ExitCode -eq 0 -and $has) "The $kind alias has a ProxyCommand line." "ssh -G for the $kind alias shows no ProxyCommand line (exit $($g.ExitCode))."
      $s = Invoke-S02Ssh -Alias $alias -Command 'true # s02:true' -TimeoutSeconds 120
      Write-S02Line "$label ssh true exit: $($s.ExitCode); elapsed $([int]$s.ElapsedSeconds) s"
      Assert-S02 ($s.ExitCode -eq 0 -and -not $s.TimedOut) "One real ssh to the $kind alias answered, exit 0." "ssh <$kind alias> true exited $($s.ExitCode)."
    }
  })

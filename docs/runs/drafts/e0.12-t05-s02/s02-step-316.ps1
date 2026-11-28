# Step 316 of E0.12-T05-S02 (OPTIONAL, for the dev lead or the architect to approve): `agentspace-operator sync -Host agentspace -Platform
# windows/amd64` run by hand under the held lease, to print the install's `Installed:` and `Patch run:` lines, which a detached provisioning run
# does not keep (operator.md, provision, "Its log"). The install is idempotent: it restarts sshd only when sshd_config's bytes changed, and the
# first boot's copy of the lines keeps them from changing. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
# It MUST run before step 315b and before Terry's sign-in, since `sync` pushes and installs under his hands otherwise.
#
# The expected lines are box/windows/install.ps1's, in its order (operator.md, What the install does, steps 6 to 10), for a host that is
# installed already: a second run reports `(already there)`.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '316' 'OPTIONAL: sync by hand, to read the install''s Installed: lines')) { exit 5 }
if (-not (Test-S02State 'lease-w.alias')) { Stop-S02Step 'no lease alias state file; step 312 has not run.'; exit 5 }
if (Test-S02State 'signin-asked.flag') { Stop-S02Step 'Terry has been asked to sign in; no sync during or after his sign-in session.'; exit 5 }
exit (Invoke-S02Main {
    $r = Invoke-S02Tool -Tool 'operator' -Arguments @('sync', '-Host', 'agentspace', '-Platform', 'windows/amd64') -TimeoutSeconds 1500
    Write-S02Line "sync exit: $($r.ExitCode); elapsed $([int]$r.ElapsedSeconds) s"
    $text = $r.Stdout + "`n" + $r.Stderr
    foreach ($line in ($text -split "\r?\n")) { if ($line -ne '') { Write-S02Line ('  ' + $line) } }
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'sync exited 0.' "sync exited $($r.ExitCode)."
    $want = @(
      @('step 6', '(?m)^Installed: the machine PATH holds agentspace''s and agent''s tool folders \(already there\)\s*$'),
      @('step 7', '(?m)^Installed: the login shell is C:\\Program Files\\Git\\bin\\bash\.exe\s*$'),
      @('step 8', '(?m)^Installed: idle check \(task \w+\), patch at boot \(task \w+\)\s*$'),
      @('step 9, sshd_config', '(?m)^Installed: sshd_config holds the agentspace lines \(already there\)\s*$'),
      @('step 9, the CA line', '(?m)^Installed: SSH trusts the agentspace CA \(.+\), password login closed, agent the only login\s*$'),
      @('step 9, the grace line', '(?m)^Installed: login grace time 30 seconds; (this sshd has no PerSourcePenalties|127\.0\.0\.1 and ::1 exempt from sshd penalties)\s*$'),
      @('step 9, the keepalive line', '(?m)^Installed: a session whose client has gone is dropped about 90 seconds after it goes quiet \(probe every 15 seconds, 5 unanswered\)\s*$'),
      @('step 10', '(?m)^Patch run: (finished this boot; not started again|already running this boot; not started again|started for this boot, as C:\\ProgramData\\agentspace\\patched is missing)\s*$')
    )
    foreach ($w in $want) { Assert-S02 ($text -cmatch $w[1]) "The install printed its $($w[0]) line." "The install printed no $($w[0]) line as install.ps1 writes it." }
    Assert-S02 (-not ($text -cmatch '(?m)^install\.ps1: ')) 'The install printed no install.ps1: error line.' 'The install printed an install.ps1: error line.'
  })

# Step 301 of E0.12-T05-S02: the AWS sign-in, by exit code alone. Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing. `aws sts get-caller-identity` runs with its stdout and stderr
# discarded inside this script (the Process handle's streams are read into memory and dropped, never printed), so
# nothing of the identity reaches .out or .err.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '301' 'the AWS sign-in, by exit code alone')) { exit 5 }
exit (Invoke-S02Main {
    $r = Invoke-S02Tool -Tool 'aws' -Arguments @('sts', 'get-caller-identity') -TimeoutSeconds 60
    $r = [pscustomobject]@{ ExitCode = $r.ExitCode; TimedOut = $r.TimedOut }
    Write-S02Line "aws sts get-caller-identity exit: $($r.ExitCode)"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'The AWS sign-in holds (exit 0).' "The AWS sign-in does not hold (exit $($r.ExitCode)). The card goes to Needs Terry through the architect: the request names 'aws login --remote' and says to paste its link into a fresh Incognito window, with every other Incognito window closed first."
  })

# Step 201: the AWS sign-in by exit code alone. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. `aws sts get-caller-identity` has both its streams discarded here, so
# nothing of it reaches .out (docs\architect.md:625), and nothing but its exit code is read.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '201' 'the AWS sign-in, by exit code alone'
    if ($null -ne $stop) { return $stop }
    Write-S01 'Command: aws sts get-caller-identity > $null (both streams discarded; only the exit code is read)'
    $run = Invoke-S01Tool -Tool 'aws' -Arguments @('sts', 'get-caller-identity') -TimeoutSec 60
    if ($run.StartFailed) { Write-S01 'FINDING: The aws program could not be started.'; return 1 }
    if ($run.TimedOut) { Write-S01 'FINDING: aws sts get-caller-identity timed out.'; return 1 }
    Write-S01 "Exit: $($run.ExitCode)"
    if ($run.ExitCode -ne 0) {
      Write-S01 "FINDING: The AWS sign-in is not good (exit $($run.ExitCode)). The card goes to Needs Terry through the architect: aws login --remote, the link it prints pasted into a fresh Incognito window, with every other Incognito window closed first."
      return 1
    }
    Write-S01 'AWS sign-in: good (exit 0).'
    return (Complete-S01 (New-S01List))
  })

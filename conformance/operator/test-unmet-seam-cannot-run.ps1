#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the test seam's last rule (docs/operator.md, "The test seam", "What each one does"): a
# program variable that names something that cannot be run MUST NOT fall back to the default, and fails the
# command with `agentspace: <variable> names <value>, which cannot be run.` and exit 1, as the program is first
# needed. Card #0222 (E0.10-T04-S02), for defect #0227: `bootstrap`'s ssh answered such a file with `agentspace:
# Could not run <path>.`, the message of a default program that cannot be started, and never with the seam's line.
#
# EXPECTED TO FAIL AGAINST v0.09's OPERATOR, whose `bootstrap` answers a file that is not a program with `Could
# not run`. A v0.10 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home with the stand-ins of
# conformance/operator/host-provision-common.ps1, a registry whose box record holds an instance ID, and runs
# `bootstrap` (the box's, with no -Host), which starts the host first (one `ec2 describe-instances` of the
# stand-in `aws`, a running instance launched long ago, so no wait) and then starts one ssh. One variable is
# then set to a value that names nothing runnable, and the case reads the exit code, both streams and the
# stand-ins' calls:
#   - `AGENTSPACE_SSH_CMD` naming a text file that is named `ssh.exe` (a file that exists and is not a program, the
#     case of #0227), a file that does not exist and a folder. The ssh is the program first needed after the start:
#     the stand-in `aws` saw the start's calls, and no stand-in `ssh` or `scp` ran.
#   - `AGENTSPACE_AWS_CMD` naming such a text file: `bootstrap`'s first need is the AWS CLI, so the same line
#     is for that variable, and no stand-in ran.
#
# SAFETY. The check runs from the wrapper's error mode (0x8001) and changes none: a file named `.exe` that is not
# a program MUST NOT raise a modal "Unsupported 16-Bit Application" dialog (#0225), and the operator's own `main`
# sets the mode first. A run that raised one is a finding, and the case's run would hang until its timeout. The
# non-program file is written inside the throwaway home through Assert-ConformancePath, and never started by the
# check. The throwaway home, the AWS gate and the stand-ins are the ones of test-unmet-host-provision.ps1. A
# failure reason never quotes a path.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-seam-cannot-run.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#the-test-seam'

# $C: N, Variable, Kind (text, missing or folder) and Runs (the stand-in `aws` calls the run is expected to make
# before the program is first needed: at least 1 for the ssh, none for the AWS CLI itself).
function Test-CannotRun($C) {
  Invoke-HpInHome -Name "${sec}: $($C.N)" -Body {
    param($h, $s)
    Initialize-HpHome -H $h
    Set-HpScenario -H $h -S $s -LockProject 'swimcoach' -Scenario @{ aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) }; rules = @() }
    $np = Join-Path $h.Root "nonprogram\$(if ($C.Variable -ceq 'AGENTSPACE_SSH_CMD') { 'ssh' } else { 'aws' }).exe"
    switch ($C.Kind) {
      'text' { Write-HpFile -H $h -Path $np -Text "This file is text. It is named like a program, and it is not one.`r`n" }
      'folder' { New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $np) | Out-Null }
      default { }
    }
    $ex = @{}; $ex[$C.Variable] = $np
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap') -Extra $ex -Timeout 120
    $clock.Stop()
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($r.TimedOut -or $clock.Elapsed.TotalSeconds -ge 100) { $p.Add('The run did not end on its own: a modal dialog MAY be waiting for a person') }
    $want = "agentspace: $($C.Variable) names $np, which cannot be run."
    if ($lines.Count -eq 0 -or $lines[-1] -cne $want) { $p.Add("The last line of stderr is not agentspace: $($C.Variable) names <the value>, which cannot be run.") }
    if ($r.Stderr -match 'Could not run') { $p.Add('Stderr holds a Could not run line, the message of a default program that cannot be started') }
    if ($r.Stdout.Trim().Length -gt 0) { $p.Add('Stdout is not empty') }
    $aws = @($calls | Where-Object { $_['kind'] -eq 'aws' })
    if ($C.Runs -eq 0 -and $aws.Count -ne 0) { $p.Add('The stand-in aws was called, though AGENTSPACE_AWS_CMD named something that cannot be run') }
    if ($C.Runs -gt 0 -and $aws.Count -lt 1) { $p.Add('The stand-in aws was never called: the start comes before the ssh, so the run did not reach the program first needed') }
    if (@($calls | Where-Object { $_['kind'] -in 'ssh', 'scp' }).Count -gt 0) { $p.Add('A stand-in ssh or scp was started: the variable named something that cannot be run, and the operator MUST NOT fall back to another program') }
    Write-HpProblems "${sec}: $($C.N): exit 1 with $($C.Variable) names <value>, which cannot be run. as the last line of stderr, no Could not run line, and no other program started (What each one does)" $p
  }
}
Test-CannotRun @{ N = 'bootstrap, AGENTSPACE_SSH_CMD naming a file that exists and is not a program (#0227)'; Variable = 'AGENTSPACE_SSH_CMD'; Kind = 'text'; Runs = 1 }
Test-CannotRun @{ N = 'bootstrap, AGENTSPACE_SSH_CMD naming a file that does not exist'; Variable = 'AGENTSPACE_SSH_CMD'; Kind = 'missing'; Runs = 1 }
Test-CannotRun @{ N = 'bootstrap, AGENTSPACE_SSH_CMD naming a folder'; Variable = 'AGENTSPACE_SSH_CMD'; Kind = 'folder'; Runs = 1 }
Test-CannotRun @{ N = 'bootstrap, AGENTSPACE_AWS_CMD naming a file that exists and is not a program'; Variable = 'AGENTSPACE_AWS_CMD'; Kind = 'text'; Runs = 0 }

Exit-ConformanceRun

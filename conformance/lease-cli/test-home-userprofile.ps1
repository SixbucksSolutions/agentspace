#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 4): the lease CLI finds
# the home folder through USERPROFILE, and every default folder follows it. It enforces:
#   - docs/state.md, "The home folder": the home folder is the value of `USERPROFILE`, and agentspace
#     MUST read it from that variable; `<state folder>` is `AGENTSPACE_STATE_DIR` when set and not
#     empty, and `%USERPROFILE%\.agentspace` otherwise; `<ssh folder>` is `AGENTSPACE_SSH_DIR` when set
#     and not empty, and `%USERPROFILE%\.ssh` otherwise.
#   - docs/state.md, "What moves which files": `USERPROFILE` moves `.agentspace` and all in it, and
#     `.ssh`; the lease folders and the CA are under it whatever the state folder is.
#   - docs/lease-api.md, "Environment": a variable that is unset or empty means the default.
#   - docs/state.md, "Requirements the prototype does not meet yet" (every part of agentspace finds the
#     home folder through USERPROFILE).
# The harness already points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home, so
# with the two folder variables unset, and again with them empty, every file lands under that home:
# the state file in `.agentspace`, the lease folder in `.agentspace\leases`, the alias file and the
# config in `.ssh`.
#
# - NOT SHOWN, and why. The other parts of agentspace named in the requirement (the operator's
#   `deploy` and `sync`, which use the shell's home variable in the prototype, and the keeper task's
#   launcher, which uses the profile folder the operating system reports) cannot be seen from outside
#   without running the real `go`, `ssh-keygen` and Task Scheduler, which a check MUST NOT do
#   (operator.md Q30). `ca` is shown by every check that makes its CA, and by test-ca.ps1. The harness
#   refuses an override of USERPROFILE, so a run with it unset is not made here either.
# - Every write is the program's; the check writes and deletes nothing. A lease ID or alias read from
#   the program's output names a file only after the document passed its schema (Get-CcLeaseDir,
#   Get-CcAliasPath). A failure reason never quotes the program's output.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process.
#   It reads no real folder, loads no prototype code and names no prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-home-userprofile.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#the-home-folder'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: With no state or ssh folder variable, every file is under USERPROFILE" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $homeState = Join-Path $lab.UserProfile '.agentspace\leases.json'
    $ids = @()
    foreach ($case in @(
        @{ Name = 'unset'; Env = @{ AGENTSPACE_STATE_DIR = $null; AGENTSPACE_SSH_DIR = $null } }
        @{ Name = 'empty'; Env = @{ AGENTSPACE_STATE_DIR = ''; AGENTSPACE_SSH_DIR = '' } }
      )) {
      $l = New-CcReadyLease -Lab $lab -Project "home-$($case.Name)" -EnvVars $case.Env
      if (-not $l.Ok) {
        Write-ConformanceResult -Name "$sec`: With the state and ssh folder variables $($case.Name), a ready create" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates"
        continue
      }
      $id = [string]$l.Doc['id']
      $alias = [string]$l.Doc['ssh']['host_alias']
      $ids += $id
      $p = [System.Collections.Generic.List[string]]::new()
      if (-not (Test-Path -LiteralPath $homeState -PathType Leaf)) { $p.Add('There is no state file at %USERPROFILE%\.agentspace\leases.json') }
      foreach ($f in 'id_ed25519', 'id_ed25519.pub', 'id_ed25519-cert.pub') {
        if (-not (Test-Path -LiteralPath (Join-Path (Get-CcLeaseDir $lab $id) $f) -PathType Leaf)) { $p.Add("There is no $f in the lease folder under %USERPROFILE%\.agentspace\leases") }
      }
      if (-not (Test-Path -LiteralPath (Get-CcAliasPath $lab $alias) -PathType Leaf)) { $p.Add('There is no alias file in %USERPROFILE%\.ssh\agentspace.d') }
      if (-not (Test-Path -LiteralPath (Join-Path $lab.SshDir 'config') -PathType Leaf)) { $p.Add('There is no config in %USERPROFILE%\.ssh') }
      Complete-ConformanceCheck -Name "$sec`: With the state and ssh folder variables $($case.Name), a ready create writes the state file, the lease folder, the alias file and the config under USERPROFILE" -Problems $p
    }
    if ($ids.Count -eq 2) {
      $doc = if (Test-Path -LiteralPath $homeState -PathType Leaf) { ConvertFrom-CcJson ([IO.File]::ReadAllText($homeState, [Text.UTF8Encoding]::new($false))) } else { $null }
      $p = [System.Collections.Generic.List[string]]::new()
      if ($null -eq $doc) { $p.Add('The default state file cannot be read') }
      else {
        $have = @($doc['leases'] | ForEach-Object { $_['id'] })
        foreach ($i in $ids) { if ($have -cnotcontains $i) { $p.Add('A lease is missing from the default state file') } }
        if ($have.Count -ne 2) { $p.Add("The default state file holds $($have.Count) records, not 2") }
      }
      Complete-ConformanceCheck -Name "$sec`: The unset and the empty variables name the same default state folder: both leases are in %USERPROFILE%\.agentspace\leases.json" -Problems $p

      # Each reads back from the default, and is unknown to the harness's own state folder.
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', $ids[0]) -EnvVars @{ AGENTSPACE_STATE_DIR = $null; AGENTSPACE_SSH_DIR = $null }
      Complete-ConformanceCheck -Name "$sec`: With the variables unset, get finds the lease in the default state folder" -Problems $(if ((Test-CcSuccess $run) -and $run.Json['id'] -ceq $ids[0]) { $null } else { [System.Collections.Generic.List[string]]@('get did not print the lease') })
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', $ids[0])
      Complete-ConformanceCheck -Name "$sec`: With AGENTSPACE_STATE_DIR set to another folder, the same lease is not found there: get is not_found" -Problems (Get-HardeningErrorProblems -Run $run -Code 'not_found' -LeaseId $ids[0])
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-home-userprofile.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

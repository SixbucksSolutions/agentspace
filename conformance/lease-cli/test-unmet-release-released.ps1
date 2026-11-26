#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041, point 3), a MUST: `release` of a `released` lease MUST delete whatever is
# left of the lease's key, certificate and alias, change nothing else, and print the lease with its
# first `released_at`. A retried release MUST finish what a failed one left.
#   - docs/lease-api.md, "agentspace lease release" (the `released` row, and "Release is
#     idempotent") and "Concurrency and atomicity".
#   - docs/state.md, "Release, and what it leaves" (What a failed release MUST leave) and
#     "Requirements the prototype does not meet yet".
# Card: E0.04-T03-S03 (#0086), carrying #0041 point 3. What the prototype does instead: release of a
# `released` record returns it unchanged and touches no file.
#
# How it gets there without a way to make a release fail: a ready lease is made by create, its alias
# file and lease folder are copied to a folder in the throwaway home, the lease is released, the
# copies are put back (so the files are what a failed release would have left), and release runs
# again. Both MUST then be gone.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - Every copy it makes goes through Assert-ConformancePath, on a path built from an ID and an alias
#   that passed the schema and their patterns. It deletes nothing itself.
# - A failure reason never quotes the program's output.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-release-released.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')

$script:UnmetProxy = 'agentspace-conformance-proxy --not-run'
$script:UnmetUuid4 = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'
$script:UnmetAlias = '\Aagentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}\z'

function Invoke-UnmetLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments)
  Invoke-AgentspaceCli -ConformanceHome $Lab -Arguments (@('lease') + $Arguments) -Environment @{ AGENTSPACE_PROXY_COMMAND = $script:UnmetProxy }
}

function Add-UnmetResult {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason (ConvertTo-ConformanceMasked ($Problems -join '; ')) }
}

# The problems with a run that was meant to succeed: exit 0, nothing on stderr, and one document on
# stdout that validates against the lease schema. It never quotes either stream.
function Get-UnmetSuccessProblems {
  param([Parameter(Mandatory)]$Run)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -ne 0) { $p.Add("Exit $($Run.ExitCode), not 0") }
  if ($Run.StderrBytes.Length -ne 0) { $p.Add("Stderr holds $($Run.StderrBytes.Length) bytes") }
  if ($Run.Json -isnot [hashtable] -or -not (Test-ConformanceSchema -Json $Run.Stdout.TrimEnd("`n") -Schema 'lease.output.schema.json')) { $p.Add('Stdout is not a lease document that validates') }
  return , $p
}

function New-UnmetReadyLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project)
  $r = [pscustomobject]@{ Ok = $false; Id = ''; Alias = ''; Why = '' }
  # Under D54 a create is ready only for a project with a ready host (E0.07-T04-S01).
  [void](Add-ConformanceReadyHost -ConformanceHome $Lab -Project $Project)
  $run = Invoke-UnmetLease $Lab @('create', '--project', $Project)
  if ($run.ExitCode -ne 0) { $r.Why = "create exited $($run.ExitCode), not 0"; return $r }
  $doc = $run.Json
  if ($doc -isnot [hashtable] -or -not (Test-ConformanceSchema -Json $run.Stdout.TrimEnd("`n") -Schema 'lease.output.schema.json')) { $r.Why = 'create did not print a lease document that validates'; return $r }
  if ($doc['state'] -cne 'ready') { $r.Why = "create gave state '$(ConvertTo-ConformanceMasked $doc['state'])', not ready"; return $r }
  $alias = [string]$doc['ssh']['host_alias']
  if ($doc['id'] -cnotmatch $script:UnmetUuid4 -or $alias -cnotmatch $script:UnmetAlias) { $r.Why = 'the ID or the alias does not match its pattern'; return $r }
  $r.Ok = $true; $r.Id = [string]$doc['id']; $r.Alias = $alias
  return $r
}

$release = 'lease-api.md#agentspace-lease-release'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$release`: Release of a released lease deletes what is left" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $lease = New-UnmetReadyLease $lab 'again'
    if (-not $lease.Ok) {
      Write-ConformanceResult -Name "$release`: Setup, a ready lease is made by create" -Status FAIL -Reason $lease.Why
    } else {
      $folder = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ".agentspace\leases\$($lease.Id)")
      $aliasFile = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.SshDir "agentspace.d\$($lease.Alias)")
      $backup = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.Root 'copies')
      $folderCopy = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $backup 'folder')
      $aliasCopy = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $backup 'alias')
      if (-not ((Test-Path -LiteralPath $folder -PathType Container) -and (Test-Path -LiteralPath $aliasFile -PathType Leaf))) {
        Write-ConformanceResult -Name "$release`: Setup, the ready lease has a lease folder and an alias file to copy" -Status FAIL -Reason 'One of the two is missing'
      } else {
        New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $backup) | Out-Null
        Copy-Item -LiteralPath $folder -Destination (Assert-ConformancePath -ConformanceHome $lab -Path $folderCopy) -Recurse -Force
        Copy-Item -LiteralPath $aliasFile -Destination (Assert-ConformancePath -ConformanceHome $lab -Path $aliasCopy) -Force

        # The first release: the lease is ready, so it deletes the files and records released.
        $first = Invoke-UnmetLease $lab @('release', $lease.Id)
        $p = Get-UnmetSuccessProblems $first
        if ($p.Count -eq 0 -and $first.Json['state'] -cne 'released') { $p.Add("The state is '$(ConvertTo-ConformanceMasked $first.Json['state'])', not released") }
        if (Test-Path -LiteralPath $folder) { $p.Add('The lease folder is still there') }
        if (Test-Path -LiteralPath $aliasFile) { $p.Add('The alias file is still there') }
        Add-UnmetResult -Name "$release`: Setup, the first release succeeds and removes the alias file and the lease folder (state.md#release-and-what-it-leaves)" -Problems $p

        if ($p.Count -eq 0) {
          # Put the copies back, as a release that failed part way would have left them.
          Copy-Item -LiteralPath $folderCopy -Destination (Assert-ConformancePath -ConformanceHome $lab -Path $folder) -Recurse -Force
          Copy-Item -LiteralPath $aliasCopy -Destination (Assert-ConformancePath -ConformanceHome $lab -Path $aliasFile) -Force

          $second = Invoke-UnmetLease $lab @('release', $lease.Id)
          $p = Get-UnmetSuccessProblems $second
          if ($p.Count -eq 0) {
            if ($second.Json['state'] -cne 'released') { $p.Add("The state is '$(ConvertTo-ConformanceMasked $second.Json['state'])', not released") }
            if ($second.Json['released_at'] -cne $first.Json['released_at']) { $p.Add('released_at is not the first one') }
          }
          Add-UnmetResult -Name "$release`: Release of a released lease exits 0 and prints the lease with its first released_at" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          if (Test-Path -LiteralPath $aliasFile) { $p.Add('The alias file is still there') }
          Add-UnmetResult -Name "$release`: Release of a released lease deletes the alias file a failed release left (state.md#release-and-what-it-leaves)" -Problems $p

          $p = [System.Collections.Generic.List[string]]::new()
          if (Test-Path -LiteralPath $folder) { $p.Add('The lease folder is still there') }
          Add-UnmetResult -Name "$release`: Release of a released lease deletes the lease folder a failed release left (state.md#release-and-what-it-leaves)" -Problems $p
        }
      }
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-release-released.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

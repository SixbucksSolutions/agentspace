#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041, point 3), a MUST: a `release` that fails part way MUST NOT leave a lease
# `ready` (or `expired`) with its key, certificate or alias gone. It leaves the lease as it was, or
# `released` with some of the three left behind, and a retried `release` MUST then delete whatever is
# left.
#   - docs/lease-api.md, "Concurrency and atomicity" (A failed `release`) and "agentspace lease
#     release" (the last bullet, and the `internal` row).
#   - docs/state.md, "Release, and what it leaves" (What a failed `release` MUST leave).
# Card: E0.04-T03-S03 (#0086), carrying #0041 point 3. What the prototype does instead: it deletes the
# alias file, fails to delete the lease folder, and leaves a `ready` record whose alias is gone.
#
# How a release is made to fail part way: the lease's `id_ed25519` is held open with no sharing at all
# (`FileShare.None`) for the length of the `release`, so the lease folder cannot be deleted (state.md,
# "Release, and what it leaves": a file another process holds open without sharing delete is such a
# failure). The handle is only ever on a file in the throwaway home, is opened after
# Assert-ConformancePath, and is closed in a `finally`.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - It deletes and writes nothing itself. Every path it opens or tests is built from an ID and an alias
#   that passed the schema and their patterns, and goes through Assert-ConformancePath.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-release-partial.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

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
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}

# The problems with a run that was meant to fail with $Code: its exit code, nothing on stdout, and one
# error document on stderr that validates and carries the code. It never quotes stderr.
function Get-UnmetErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [Parameter(Mandatory)][int]$Exit)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -eq 0) { $p.Add("Exit 0, not $Exit`: the command succeeded"); return , $p }
  if ($Run.ExitCode -ne $Exit) { $p.Add("Exit $($Run.ExitCode), not $Exit") }
  if ($Run.StdoutBytes.Length -ne 0) { $p.Add("Stdout holds $($Run.StdoutBytes.Length) bytes") }
  $doc = $Run.ErrorJson
  if ($doc -isnot [hashtable]) { $p.Add("Stderr ($($Run.StderrBytes.Length) bytes) is not a JSON document"); return , $p }
  if (-not (Test-ConformanceSchema -Json $Run.Stderr.TrimEnd("`n") -Schema 'error.output.schema.json')) { $p.Add('The error document does not validate against error.output.schema.json') }
  if ($doc['code'] -cne $Code) { $p.Add("The error code is '$($doc['code'])', not '$Code'") }
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
  if ($doc['state'] -cne 'ready') { $r.Why = "create gave state '$($doc['state'])', not ready"; return $r }
  $alias = [string]$doc['ssh']['host_alias']
  if ($doc['id'] -cnotmatch $script:UnmetUuid4 -or $alias -cnotmatch $script:UnmetAlias) { $r.Why = 'the ID or the alias does not match its pattern'; return $r }
  $r.Ok = $true; $r.Id = [string]$doc['id']; $r.Alias = $alias
  return $r
}

function Get-UnmetRecord {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id)
  $doc = Read-ConformanceStateFile -ConformanceHome $Lab
  if ($null -eq $doc) { return $null }
  return @($doc.leases | Where-Object { $_.id -ceq $Id })[0]
}

$release = 'lease-api.md#concurrency-and-atomicity'
$lab = $null
$held = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$release`: A release that fails part way does not leave a ready lease without its alias" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $lease = New-UnmetReadyLease $lab 'partial'
    if (-not $lease.Ok) {
      Write-ConformanceResult -Name "$release`: Setup, a ready lease is made by create" -Status FAIL -Reason $lease.Why
    } else {
      $folder = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ".agentspace\leases\$($lease.Id)")
      $key = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $folder 'id_ed25519')
      $cert = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $folder 'id_ed25519-cert.pub')
      $aliasFile = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.SshDir "agentspace.d\$($lease.Alias)")
      if (-not ((Test-Path -LiteralPath $key -PathType Leaf) -and (Test-Path -LiteralPath $cert -PathType Leaf) -and (Test-Path -LiteralPath $aliasFile -PathType Leaf))) {
        Write-ConformanceResult -Name "$release`: Setup, the ready lease has a key, a certificate and an alias file" -Status FAIL -Reason 'One of the three is missing'
      } else {
        # Hold the private key with no sharing at all, so that the lease folder cannot be deleted.
        $held = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $lab -Path $key), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
        $run = Invoke-UnmetLease $lab @('release', $lease.Id)

        # What the first release left, read while the key is still held: the folder cannot go yet.
        $rec = Get-UnmetRecord $lab $lease.Id
        $aliasThere = Test-Path -LiteralPath $aliasFile -PathType Leaf
        $keyThere = Test-Path -LiteralPath $key -PathType Leaf
        $certThere = Test-Path -LiteralPath $cert -PathType Leaf
        $held.Dispose()
        $held = $null

        $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
        Add-UnmetResult -Name "$release`: A release that cannot delete the key fails with internal, exit 1, one error document and no stdout (lease-api.md#agentspace-lease-release)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        if ($null -eq $rec) { $p.Add('The record is gone') }
        elseif ($rec.state -cne 'ready' -and $rec.state -cne 'released') { $p.Add("The stored state is '$($rec.state)', not ready or released") }
        elseif ($rec.state -ceq 'ready') {
          if (-not $aliasThere) { $p.Add('The record is ready and its alias file is gone') }
          if (-not $keyThere) { $p.Add('The record is ready and its key is gone') }
          if (-not $certThere) { $p.Add('The record is ready and its certificate is gone') }
        }
        Add-UnmetResult -Name "$release`: A release that fails part way MUST NOT leave a ready lease with its alias, key or certificate gone (#0041; state.md#release-and-what-it-leaves)" -Problems $p

        # With the key free again, a retried release finishes the job.
        $again = Invoke-UnmetLease $lab @('release', $lease.Id)
        $p = [System.Collections.Generic.List[string]]::new()
        if ($again.ExitCode -ne 0) { $p.Add("Exit $($again.ExitCode), not 0") }
        elseif ($again.Json -isnot [hashtable] -or $again.Json['state'] -cne 'released') { $p.Add('The retried release did not print the lease released') }
        $recAfter = Get-UnmetRecord $lab $lease.Id
        if ($null -eq $recAfter -or $recAfter.state -cne 'released') { $p.Add('The record is not released after the retry') }
        if (Test-Path -LiteralPath $aliasFile) { $p.Add('The alias file is still there after the retry') }
        if (Test-Path -LiteralPath $folder) { $p.Add('The lease folder is still there after the retry') }
        Add-UnmetResult -Name "$release`: A retried release MUST finish the job: the lease is released, with no alias file and no lease folder left (lease-api.md#agentspace-lease-release)" -Problems $p
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-release-partial.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($held) { $held.Dispose() }
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

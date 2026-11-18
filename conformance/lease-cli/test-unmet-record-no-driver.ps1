#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041, point 2), MUSTs: a stored `ready` record with no `driver` member MUST be
# read as placed by the v0 default driver, `ec2`, so `renew` and `release` MUST hand it to that driver
# rather than fail; and they MUST NOT add `driver` to a record that lacks it.
#   - docs/state.md, "The lease state file" (The driver is kept for as long as the lease is: Reading,
#     and the rule that `renew` and `release` MUST NOT add `driver`).
#   - docs/lease-api.md, "agentspace lease renew" and "agentspace lease release".
# Card: E0.04-T03-S03 (#0086), carrying #0041 point 2. What the prototype does instead: it fails both
# `renew` and `release` of such a record with `internal`.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - Its only writes are to the throwaway home's state file, through Write-ConformanceStateFile, which
#   calls Assert-ConformancePath. It deletes nothing itself. A path built from the program's output
#   (a lease ID, a host alias) is built only after the document passed its schema and the ID and alias
#   patterns, and then only through Assert-ConformancePath.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-record-no-driver.ps1
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

# A ready lease made by create. The ID and the alias are set only when the document validates and
# both match their patterns, so a path built from them is safe to build.
function New-UnmetReadyLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Project)
  $r = [pscustomobject]@{ Ok = $false; Id = ''; Alias = ''; Why = '' }
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

# The record with this ID in the state file, or $null.
function Get-UnmetRecord {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id)
  $doc = Read-ConformanceStateFile -ConformanceHome $Lab
  if ($null -eq $doc) { return $null }
  return @($doc.leases | Where-Object { $_.id -ceq $Id })[0]
}

function Test-UnmetHasDriver {
  param([Parameter(Mandatory)]$Record)
  return ($Record.PSObject.Properties.Name -contains 'driver')
}

# Removes the `driver` member from one record, as an older record would be. Returns a reason when it
# cannot, or '' when the record then has none and the state file still validates against its schema.
function Remove-UnmetDriver {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Id)
  $doc = Read-ConformanceStateFile -ConformanceHome $Lab
  if ($null -eq $doc) { return 'There is no state file' }
  $rec = @($doc.leases | Where-Object { $_.id -ceq $Id })[0]
  if ($null -eq $rec) { return 'The state file has no record of the lease' }
  if (-not (Test-UnmetHasDriver $rec)) { return 'The record has no driver member to remove' }
  [void]$rec.PSObject.Properties.Remove('driver')
  Write-ConformanceStateFile -ConformanceHome $Lab -Document $doc
  $after = Get-UnmetRecord $Lab $Id
  if ($null -eq $after -or (Test-UnmetHasDriver $after)) { return 'The record still has a driver member' }
  $text = [IO.File]::ReadAllText((Assert-ConformancePath -ConformanceHome $Lab -Path (Get-ConformanceStatePath -ConformanceHome $Lab)))
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/leases.state.schema.json')) { return 'The state file no longer validates against its schema' }
  return ''
}

$state = 'state.md#the-lease-state-file'
$renew = 'lease-api.md#agentspace-lease-renew'
$release = 'lease-api.md#agentspace-lease-release'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$state`: A ready record with no driver is renewed and released" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    # ---- renew -------------------------------------------------------------------------------------
    $a = New-UnmetReadyLease $lab 'nodriver-a'
    if (-not $a.Ok) {
      Write-ConformanceResult -Name "$state`: Setup, a ready lease is made by create for the renew" -Status FAIL -Reason $a.Why
    } else {
      $why = Remove-UnmetDriver $lab $a.Id
      if ($why) {
        Write-ConformanceResult -Name "$state`: Setup, the renew's record has its driver member removed and still validates" -Status FAIL -Reason $why
      } else {
        Write-ConformanceResult -Name "$state`: Setup, the renew's record has no driver member and the state file validates" -Status PASS
        $run = Invoke-UnmetLease $lab @('renew', $a.Id)
        $p = Get-UnmetSuccessProblems $run
        if ($p.Count -eq 0) {
          if ($run.Json['id'] -cne $a.Id) { $p.Add('The lease printed is not the one renewed') }
          if ($run.Json['state'] -cne 'ready') { $p.Add("The state is '$($run.Json['state'])', not ready") }
        }
        Add-UnmetResult -Name "$renew`: A stored ready record with no driver is read as ec2, so renew exits 0 and prints the lease (state.md#the-lease-state-file, Reading)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        $rec = Get-UnmetRecord $lab $a.Id
        if ($null -eq $rec) { $p.Add('The record is gone') }
        elseif (Test-UnmetHasDriver $rec) { $p.Add('renew added a driver member to a record that had none') }
        Add-UnmetResult -Name "$renew`: renew MUST NOT add a driver member to a record that has none (state.md#the-lease-state-file)" -Problems $p
      }
    }

    # ---- release -----------------------------------------------------------------------------------
    $b = New-UnmetReadyLease $lab 'nodriver-b'
    if (-not $b.Ok) {
      Write-ConformanceResult -Name "$state`: Setup, a ready lease is made by create for the release" -Status FAIL -Reason $b.Why
    } else {
      $why = Remove-UnmetDriver $lab $b.Id
      if ($why) {
        Write-ConformanceResult -Name "$state`: Setup, the release's record has its driver member removed and still validates" -Status FAIL -Reason $why
      } else {
        Write-ConformanceResult -Name "$state`: Setup, the release's record has no driver member and the state file validates" -Status PASS
        $folder = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ".agentspace\leases\$($b.Id)")
        $aliasFile = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.SshDir "agentspace.d\$($b.Alias)")
        $run = Invoke-UnmetLease $lab @('release', $b.Id)
        $p = Get-UnmetSuccessProblems $run
        if ($p.Count -eq 0) {
          if ($run.Json['id'] -cne $b.Id) { $p.Add('The lease printed is not the one released') }
          if ($run.Json['state'] -cne 'released') { $p.Add("The state is '$($run.Json['state'])', not released") }
        }
        Add-UnmetResult -Name "$release`: A stored ready record with no driver is read as ec2, so release exits 0 and prints the lease released (state.md#the-lease-state-file, Reading)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        $rec = Get-UnmetRecord $lab $b.Id
        if ($null -eq $rec) { $p.Add('The record is gone') }
        else {
          if ($rec.state -cne 'released') { $p.Add("The stored state is '$($rec.state)', not released") }
          if (Test-UnmetHasDriver $rec) { $p.Add('release added a driver member to a record that had none') }
        }
        Add-UnmetResult -Name "$release`: release records the lease released and MUST NOT add a driver member (state.md#the-lease-state-file)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        if (Test-Path -LiteralPath $aliasFile) { $p.Add('The alias file is still there') }
        if (Test-Path -LiteralPath $folder) { $p.Add('The lease folder is still there') }
        Add-UnmetResult -Name "$release`: release of a ready record with no driver deletes the alias file and the lease folder (state.md#release-and-what-it-leaves)" -Problems $p
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-record-no-driver.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

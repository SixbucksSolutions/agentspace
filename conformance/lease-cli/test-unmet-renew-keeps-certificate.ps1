#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041, point 3), a MUST: a failed `renew` MUST leave the lease's `expires_at`,
# `ttl_seconds` and certificate as they were, so the previous certificate MUST be kept, byte for byte,
# until the record with the new `expires_at` is written, and put back if that write fails.
#   - docs/lease-api.md, "Concurrency and atomicity" (A failed `renew`) and "agentspace lease renew"
#     (the `internal` row).
#   - docs/state.md, "Renewal" (What a failed `renew` MUST leave) and "Replacing a file".
# Card: E0.04-T03-S03 (#0086), carrying #0041 point 3. What the prototype does instead: it moves the
# new certificate into place, then fails to write the record, and leaves the new certificate. It fails
# after a retry of about 13 seconds.
#
# How the record write is made to fail: `leases.json` is held open for reading, with no delete
# sharing, for the length of the `renew`. The state file is replaced by a rename (state.md, "Replacing
# a file"), which a file open that way refuses, so any conforming build fails the write. The handle
# is closed in a `finally`, and it is only ever on a file in the throwaway home.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - It deletes and writes nothing itself. Every path it opens or reads is built from an ID that passed
#   the schema and the ID pattern, and goes through Assert-ConformancePath.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-renew-keeps-certificate.ps1
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

$renew = 'lease-api.md#concurrency-and-atomicity'
$lab = $null
$held = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$renew`: A failed renew keeps the previous certificate" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $lease = New-UnmetReadyLease $lab 'keepcert'
    if (-not $lease.Ok) {
      Write-ConformanceResult -Name "$renew`: Setup, a ready lease is made by create" -Status FAIL -Reason $lease.Why
    } else {
      $certPath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile ".agentspace\leases\$($lease.Id)\id_ed25519-cert.pub")
      $statePath = Assert-ConformancePath -ConformanceHome $lab -Path (Get-ConformanceStatePath -ConformanceHome $lab)
      if (-not ((Test-Path -LiteralPath $certPath -PathType Leaf) -and (Test-Path -LiteralPath $statePath -PathType Leaf))) {
        Write-ConformanceResult -Name "$renew`: Setup, the ready lease has a certificate and a state file to hold" -Status FAIL -Reason 'One of the two is missing'
      } else {
        $certBefore = [IO.File]::ReadAllBytes($certPath)
        $recBefore = @((Read-ConformanceStateFile -ConformanceHome $lab).leases | Where-Object { $_.id -ceq $lease.Id })[0]
        $expiresBefore = [string]$recBefore.expires_at
        $ttlBefore = [int]$recBefore.ttl_seconds

        # Hold the state file for reading with no delete sharing, so that its replacement fails.
        $held = [IO.FileStream]::new($statePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $run = Invoke-UnmetLease $lab @('renew', $lease.Id, '--ttl', '90m')
        $held.Dispose()
        $held = $null

        $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
        Add-UnmetResult -Name "$renew`: A renew whose record cannot be written fails with internal, exit 1, one error document and no stdout (lease-api.md#agentspace-lease-renew)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        if (-not (Test-Path -LiteralPath $certPath -PathType Leaf)) { $p.Add('The certificate is gone') }
        else {
          $certAfter = [IO.File]::ReadAllBytes($certPath)
          if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$certAfter, [byte[]]$certBefore)) { $p.Add("The certificate is not the previous one, byte for byte: $($certBefore.Length) bytes before, $($certAfter.Length) after") }
        }
        Add-UnmetResult -Name "$renew`: A failed renew MUST keep the previous certificate byte for byte (state.md#renewal, What a failed renew MUST leave)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        $recAfter = @((Read-ConformanceStateFile -ConformanceHome $lab).leases | Where-Object { $_.id -ceq $lease.Id })[0]
        if ($null -eq $recAfter) { $p.Add('The record is gone') }
        else {
          if ([string]$recAfter.expires_at -cne $expiresBefore) { $p.Add('expires_at changed') }
          if ([int]$recAfter.ttl_seconds -ne $ttlBefore) { $p.Add('ttl_seconds changed') }
          if ([string]$recAfter.state -cne 'ready') { $p.Add("The stored state is '$($recAfter.state)', not ready") }
        }
        Add-UnmetResult -Name "$renew`: A failed renew MUST leave expires_at and ttl_seconds as they were" -Problems $p
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-renew-keeps-certificate.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($held) { $held.Dispose() }
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

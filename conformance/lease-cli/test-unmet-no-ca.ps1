#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0029, point 3), a MUST: `create` and `renew` MUST NOT create agentspace's CA, and
# MUST fail with `internal`, change nothing and make no CA when there is none.
#   - docs/lease-api.md, "The credential" (The certificate authority) and "Messages" (No CA).
#   - docs/state.md, "The CA" (Its checks) and "Creation, in order" (step 1).
# Card: E0.04-T03-S03 (#0086), carrying #0029 point 3. What the prototype does instead: it checks the
# CA by running the operator's `ca`, which makes one when there is none, so a ready `create` succeeds
# and `renew` succeeds, each after making a CA that no host trusts.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - The only delete it makes is the throwaway home's own CA folder, through Assert-ConformancePath.
#   It builds a path from the program's output only after the document has passed its schema and the
#   UUID and alias patterns, and then only through Assert-ConformancePath.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-no-ca.ps1
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

# A ready lease made by create. Returns Ok, Id and Alias; the ID and the alias are set only when the
# document validates and both match their patterns, so a path built from them is safe to build.
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

# The records in the state file: 0 when there is no file, -1 when the file is not a JSON document.
function Get-UnmetRecordCount {
  param([Parameter(Mandatory)]$Lab)
  $path = Join-Path $Lab.StateDir 'leases.json'
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return 0 }
  try { return @(([IO.File]::ReadAllText($path) | ConvertFrom-Json -Depth 50 -ErrorAction Stop).leases).Count } catch { return -1 }
}

function Get-UnmetFileNames {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return @() }
  return @(Get-ChildItem -LiteralPath $Path -Force | ForEach-Object Name | Sort-Object)
}

# Every file under the home's .agentspace and .ssh, as relative path to SHA-256, leaving out the CA
# (checked on its own) and leases.lock (an empty file every lease command may make, state.md "The lease state lock").
function Get-UnmetFileMap {
  param([Parameter(Mandatory)]$Lab)
  $map = @{}
  $base = [IO.Path]::GetFullPath($Lab.UserProfile).TrimEnd('\') + '\'
  foreach ($top in '.agentspace', '.ssh') {
    $dir = Join-Path $Lab.UserProfile $top
    if (-not (Test-Path -LiteralPath $dir -PathType Container)) { continue }
    foreach ($f in Get-ChildItem -LiteralPath $dir -Recurse -File -Force) {
      $rel = $f.FullName.Substring($base.Length)
      if ($rel -like '.agentspace\ca\*' -or $rel -ieq '.agentspace\state\leases.lock') { continue }
      $map[$rel] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash
    }
  }
  return $map
}

# The differences between two maps, as relative paths (each is under the throwaway home, and holds no name).
function Get-UnmetMapDifferences {
  param([Parameter(Mandatory)]$Before, [Parameter(Mandatory)]$After)
  $d = [System.Collections.Generic.List[string]]::new()
  foreach ($k in $Before.Keys) {
    if (-not $After.ContainsKey($k)) { $d.Add("$k is gone") } elseif ($After[$k] -ne $Before[$k]) { $d.Add("$k changed") }
  }
  foreach ($k in $After.Keys) { if (-not $Before.ContainsKey($k)) { $d.Add("$k is new") } }
  return , $d
}

function Remove-UnmetCa {
  param([Parameter(Mandatory)]$Lab)
  $caDir = Assert-ConformancePath -ConformanceHome $Lab -Path (Join-Path $Lab.UserProfile '.agentspace\ca')
  if (Test-Path -LiteralPath $caDir) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $caDir) -Recurse -Force }
}

$create = 'lease-api.md#the-credential'
$renew = 'lease-api.md#the-credential'
$labA = $null
$labB = $null
try {
  # ---- create with no CA -------------------------------------------------------------------------
  $labA = New-ConformanceHome
  if (-not $labA.CaReady) {
    Write-ConformanceResult -Name "$create`: With no CA, a ready create fails with internal" -Status SKIPPED -Reason "No throwaway CA could be made, so there is none to remove: $($labA.CaReason)"
  } else {
    Remove-UnmetCa $labA
    $run = Invoke-UnmetLease $labA @('create', '--project', 'noca')
    $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
    Add-UnmetResult -Name "$create`: With no CA, a ready create fails with internal, exit 1, one error document and no stdout (state.md#creation-in-order step 1)" -Problems $p

    $p = [System.Collections.Generic.List[string]]::new()
    if (Test-Path -LiteralPath (Join-Path $labA.UserProfile '.agentspace\ca')) { $p.Add('create made a CA folder') }
    Add-UnmetResult -Name "$create`: With no CA, create MUST NOT make one (state.md#its-checks)" -Problems $p

    $p = [System.Collections.Generic.List[string]]::new()
    $n = Get-UnmetRecordCount $labA
    if ($n -ne 0) { $p.Add($(if ($n -lt 0) { 'The state file is not a JSON document' } else { "The state file holds $n records" })) }
    $leases = Get-UnmetFileNames (Join-Path $labA.UserProfile '.agentspace\leases')
    if ($leases.Count -gt 0) { $p.Add("There are $($leases.Count) entries under the leases folder") }
    $aliases = Get-UnmetFileNames (Join-Path $labA.SshDir 'agentspace.d')
    if ($aliases.Count -gt 0) { $p.Add("There are $($aliases.Count) alias files") }
    Add-UnmetResult -Name "$create`: With no CA, a failed create leaves no record, no lease folder and no alias file (lease-api.md#concurrency-and-atomicity)" -Problems $p
  }

  # ---- renew with no CA --------------------------------------------------------------------------
  $labB = New-ConformanceHome
  if (-not $labB.CaReady) {
    Write-ConformanceResult -Name "$renew`: With the CA gone, renew fails with internal" -Status SKIPPED -Reason "No throwaway CA could be made: $($labB.CaReason)"
  } else {
    $lease = New-UnmetReadyLease $labB 'noca'
    if (-not $lease.Ok) {
      Write-ConformanceResult -Name "$renew`: Setup, a ready lease is made while the CA exists" -Status FAIL -Reason $lease.Why
    } else {
      $before = Get-UnmetFileMap $labB
      Remove-UnmetCa $labB
      $run = Invoke-UnmetLease $labB @('renew', $lease.Id)
      $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
      Add-UnmetResult -Name "$renew`: With the CA gone, renew fails with internal, exit 1, one error document and no stdout (state.md#renewal step 1)" -Problems $p

      $p = [System.Collections.Generic.List[string]]::new()
      if (Test-Path -LiteralPath (Join-Path $labB.UserProfile '.agentspace\ca')) { $p.Add('renew made a CA folder') }
      Add-UnmetResult -Name "$renew`: With the CA gone, renew MUST NOT make one (state.md#its-checks)" -Problems $p

      $after = Get-UnmetFileMap $labB
      $diff = Get-UnmetMapDifferences -Before $before -After $after
      $p = [System.Collections.Generic.List[string]]::new()
      if ($diff.Count -gt 0) { $p.Add("The lease was not left as it was: $($diff -join ', ')") }
      Add-UnmetResult -Name "$renew`: With the CA gone, a failed renew leaves the lease as it was: the record, the key, the certificate, the alias file and the config (lease-api.md#concurrency-and-atomicity)" -Problems $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-no-ca.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($labA) { Remove-ConformanceHome -ConformanceHome $labA }
  if ($labB) { Remove-ConformanceHome -ConformanceHome $labB }
}
Exit-ConformanceRun

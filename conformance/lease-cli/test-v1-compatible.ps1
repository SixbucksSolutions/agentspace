#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.10-T04-S01, #0221): D76's promise, that contract v1 keeps contract v0 for a
# command line with no `--plan` (docs/lease-api.md, Versioning, "What v1 keeps of v0", and Schemas). Every
# document such a command line prints, on stdout or on stderr, validates against v0's schema in `schemas/v0/` as
# well as against v1's in `schemas/`: a lease in each state it can be in (ready, pending, expired, released, unsatisfiable),
# each reason v0 had (unsupported_platform, unsupported_profile, unsupported_size, no_capacity), a list, and an
# error of each code (usage, invalid_value, out_of_range, not_found, invalid_state, internal). It drives the lease
# CLI only as a separate process, through the harness, in a throwaway home, with a stand-in driver as every other
# lease CLI check has: a stand-in provisioning run, and a host registry the check seeds. Every expectation is taken
# from docs/lease-api.md, never from a program's output.
#
# - A document is checked as the contract says (compact, one line, nothing on the other stream, the exit code of its
#   error code), against v1's schema, and then against v0's. A lease document MUST NOT hold `sized_for` or the reason
#   `plan_too_large`, and an error MUST NOT name the field `plan`, since no plan was given.
# - A last group of controls makes the check show it can fail (conformance.md, "Running the suite": a check is proved
#   able to fail): a lease made with `--plan` and an error that names the field `plan` MUST validate against v1's
#   schema and MUST NOT validate against v0's, because v0's schemas allow no member and no field they did not have.
#   Without them, a validator that accepted everything would pass this file.
# - A result at the end shows that every state, every reason and every error code above was seen, so a run that made
#   too few documents cannot pass.
# - Test-ConformanceSchema reaches a schema under schemas/ by its path under that folder, so `v0/error.output.schema.json`
#   names v0's error schema, and the harness is used as it is.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home, and every create
# here is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it is given and makes no
# host. Every write calls Assert-ConformancePath at the call. A project of the check's own has a host the check seeds,
# or none. No program of agentspace's but the lease CLI is started, and only through the harness. A failure reason
# never quotes a value the stand-in captured.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-v1-compatible.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped (no CA, for the ready cases).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$script:VcLab = $null
$script:VcStandIn = $null
$script:VcStates = @{}
$script:VcReasons = @{}
$script:VcCodes = @{}
$script:VcDocs = 0
$script:VcUuid = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'

function Format-VcText([string]$Text) {
  $x = [string]$Text
  $x = $x -replace '[A-Za-z]:[\\/][^\s''"]*', '<path>'
  $x = $x -replace '\s+', ' '
  if ($x.Length -gt 260) { $x = $x.Substring(0, 260) + '...' }
  return $x
}

function Complete-VcCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ((@($Problems | ForEach-Object { Format-VcText $_ })) -join '; ') }
}

# `agentspace <Arguments>` as typed (the first word is `lease`), with the check's ProxyCommand and the stand-in as
# the provisioning run.
function Invoke-VcCli {
  param([Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments)
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy; AGENTSPACE_PROVISION_CMD = $script:VcStandIn.Path }
  return (Invoke-AgentspaceCli -ConformanceHome $script:VcLab -Arguments $Arguments -Environment $e)
}

# The schema pairs of a kind of document: v1's under schemas/, and v0's under schemas/v0/ (lease-api.md, Schemas).
function Get-VcSchemas([string]$Kind) {
  switch ($Kind) {
    'lease' { return @{ V1 = 'lease.output.schema.json'; V0 = 'v0/lease.output.schema.json' } }
    'list' { return @{ V1 = 'lease-list.output.schema.json'; V0 = 'v0/lease-list.output.schema.json' } }
    'error' { return @{ V1 = 'error.output.schema.json'; V0 = 'v0/error.output.schema.json' } }
  }
}

# The problems with a run's document, which MUST validate against both versions' schemas. A success is judged as
# lease-api.md, Output, says; a failure as Exit codes and errors says. A document that holds something only v1 has
# is a problem: sized_for, the reason plan_too_large, the field plan.
function Get-VcProblems {
  param(
    [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][ValidateSet('lease', 'list', 'error')][string]$Kind,
    [string]$Code, [string]$Field = '', [string]$State, [string]$LeaseId = '', [string]$ErrState = '', [string[]]$Reasons
  )
  $schemas = Get-VcSchemas $Kind
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Kind -ceq 'error') {
    foreach ($x in (Get-ConformanceErrorProblems -Run $Run -Code $Code -Field $Field -LeaseId $LeaseId -State $ErrState)) { $p.Add($x) }
    $text = [string]$Run.Stderr
    $doc = $Run.ErrorJson
  } else {
    foreach ($x in (Get-ConformanceSuccessProblems -Run $Run -Schema $schemas.V1)) { $p.Add($x) }
    $text = [string]$Run.Stdout
    $doc = $Run.Json
  }
  if ($null -eq $doc) { return , $p }
  $script:VcDocs++
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas.V0)) { $p.Add("The document does not validate against $($schemas.V0), contract v0's schema") }
  $leases = @()
  if ($Kind -ceq 'lease') { $leases = @($doc) }
  elseif ($Kind -ceq 'list') { $leases = @($doc['leases']) }
  foreach ($l in $leases) {
    if ($l -isnot [hashtable]) { continue }
    if ($l.ContainsKey('sized_for')) { $p.Add('A lease made with no plan holds sized_for') }
    $script:VcStates[[string]$l['state']] = $true
    foreach ($r in @($l['reasons'] | Where-Object { $null -ne $_ })) {
      $script:VcReasons[[string]$r['code']] = $true
      if ($r['code'] -ceq 'plan_too_large') { $p.Add('A lease made with no plan has the reason plan_too_large') }
    }
  }
  if ($Kind -ceq 'lease' -and $State -and $doc['state'] -cne $State) { $p.Add("The state is '$($doc['state'])', not '$State'") }
  if ($Kind -ceq 'lease' -and $Reasons) {
    $got = @($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['code'] })
    if (($got -join ',') -cne ($Reasons -join ',')) { $p.Add("The reasons are [$($got -join ', ')], not [$($Reasons -join ', ')]") }
  }
  if ($Kind -ceq 'error') {
    $script:VcCodes[[string]$doc['code']] = $true
    if ($doc.ContainsKey('field') -and $doc['field'] -ceq 'plan') { $p.Add("An error for a line with no plan names the field 'plan'") }
  }
  return , $p
}

# Runs a line and checks its document against both versions' schemas. Returns the parsed lease or list, or $null when
# the document did not pass, so a check that goes on from it never builds on a document it has not validated.
function Test-VcLine {
  param(
    [Parameter(Mandatory)][string]$Why, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [Parameter(Mandatory)][ValidateSet('lease', 'list', 'error')][string]$Kind,
    [string]$Code, [string]$Field = '', [string]$State, [string]$LeaseId = '', [string]$ErrState = '', [string[]]$Reasons
  )
  $what = switch ($Kind) {
    'lease' { "a lease$(if ($State) { ", $State" })$(if ($Reasons) { ", reasons $($Reasons -join ' then ')" })" }
    'list' { 'a list' }
    'error' { "a $Code error$(if ($Field) { " for '$Field'" })" }
  }
  $name = "lease-api.md#versioning: $(Format-ConformanceLine $Arguments) prints $what ($Why) that validates against v0's schema and v1's"
  $p = [System.Collections.Generic.List[string]]::new()
  $result = $null
  try {
    $run = Invoke-VcCli -Arguments $Arguments
    $splat = @{ Run = $run; Kind = $Kind; Field = $Field; LeaseId = $LeaseId; ErrState = $ErrState }
    if ($Code) { $splat['Code'] = $Code }
    if ($State) { $splat['State'] = $State }
    if ($Reasons) { $splat['Reasons'] = $Reasons }
    foreach ($x in (Get-VcProblems @splat)) { $p.Add($x) }
    if ($p.Count -eq 0) { $result = if ($Kind -ceq 'error') { $run.ErrorJson } else { $run.Json } }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-VcCheck -Name $name -Problems $p
  return $result
}

function Skip-VcNoCa([string]$Name) {
  Write-ConformanceResult -Name $Name -Status SKIPPED -Reason "No throwaway CA could be made: $($script:VcLab.CaReason)"
}

# A lease ID from a document that passed its schema, or '' (so no later line is built on an ID that was never seen).
function Get-VcId($Doc) {
  if ($Doc -is [hashtable] -and [string]$Doc['id'] -cmatch $script:VcUuid) { return [string]$Doc['id'] }
  return ''
}

$lab = $null
try {
  $lab = New-ConformanceHome
  $script:VcLab = $lab
  $script:VcStandIn = New-PvStandIn -Lab $lab -Name 'v1-compatible'
  $notFound = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'
  # vc-ready has a ready host at m's type, so a default create is ready; vc-pend has a ready host whose size is not
  # recorded, which answers pending (lease-api.md, "A host's life follows its leases"); vc-nocap has no host and is not
  # on the project list, so it answers no_capacity (D54).
  [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project 'vc-ready')
  [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project 'vc-pend' -NoInstanceType)

  # --- A ready lease, and everything a command does with it ---
  $id1 = ''
  $id2 = ''
  if ($lab.CaReady) {
    $d = Test-VcLine -Why 'a ready lease' -Arguments @('lease', 'create', '--project', 'vc-ready') -Kind lease -State ready
    $id1 = Get-VcId $d
    $d = Test-VcLine -Why 'a ready lease with a TTL' -Arguments @('lease', 'create', '--project', 'vc-ready', '--ttl', '90m') -Kind lease -State ready
    $id2 = Get-VcId $d
    $d = Test-VcLine -Why 'every flag but a plan, in other cases and forms' -Arguments @('LEASE', 'Create', '--PROJECT=VC-READY', '--Platform', ' Linux/AArch64 ', '--profile=Agent-Linux', '--Size', 'M', '--TTL=1H') -Kind lease -State ready
    $id3 = Get-VcId $d
    if ($id1) {
      [void](Test-VcLine -Why 'a ready lease' -Arguments @('lease', 'get', $id1) -Kind lease -State ready)
      [void](Test-VcLine -Why 'a ready lease, the ID in upper case with spaces' -Arguments @('Lease', 'GET', "  $($id1.ToUpperInvariant())  ") -Kind lease -State ready)
      [void](Test-VcLine -Why 'a ready lease renewed with a TTL' -Arguments @('lease', 'renew', $id1, '--ttl', '2h') -Kind lease -State ready)
      [void](Test-VcLine -Why 'a ready lease renewed with its own TTL' -Arguments @('lease', 'renew', $id1) -Kind lease -State ready)
    } else { foreach ($n in 'get', 'get in other forms', 'renew with a TTL', 'renew') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a ready lease validates against v0's schema and v1's" -Status FAIL -Reason 'The create printed no ready lease that validates, so there was nothing to read back' } }
    # An expired lease: a ready lease whose expires_at has passed (state.md, the lease state file), read by each command.
    if ($id2 -and (Set-ConformanceRecordMember -ConformanceHome $lab -Id $id2 -Member 'expires_at' -Value '2020-01-01T00:00:00Z')) {
      [void](Test-VcLine -Why 'an expired lease' -Arguments @('lease', 'get', $id2) -Kind lease -State expired)
      [void](Test-VcLine -Why 'an expired lease, in a list' -Arguments @('lease', 'list', '--project', 'vc-ready', '--state', 'expired') -Kind list)
      [void](Test-VcLine -Why 'an expired lease cannot be renewed' -Arguments @('lease', 'renew', $id2) -Kind error -Code invalid_state -LeaseId $id2 -ErrState expired)
      [void](Test-VcLine -Why 'an expired lease released' -Arguments @('lease', 'release', $id2) -Kind lease -State released)
    } else { foreach ($n in 'get', 'list', 'renew', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of an expired lease validates against v0's schema and v1's" -Status FAIL -Reason 'There was no ready lease to make expired' } }
    # A released lease, released again.
    if ($id1) {
      [void](Test-VcLine -Why 'a ready lease released' -Arguments @('lease', 'release', $id1) -Kind lease -State released)
      [void](Test-VcLine -Why 'a released lease released again' -Arguments @('lease', 'release', $id1) -Kind lease -State released)
      [void](Test-VcLine -Why 'a released lease' -Arguments @('lease', 'get', $id1) -Kind lease -State released)
      [void](Test-VcLine -Why 'a released lease cannot be renewed' -Arguments @('lease', 'renew', $id1) -Kind error -Code invalid_state -LeaseId $id1 -ErrState released)
    } else { foreach ($n in 'release', 'a second release', 'get', 'renew') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a released lease validates against v0's schema and v1's" -Status FAIL -Reason 'There was no ready lease to release' } }
    if ($id3) { [void](Test-VcLine -Why 'a lease made in other cases and forms, released' -Arguments @('lease', 'release', $id3) -Kind lease -State released) }
  } else {
    foreach ($n in 'a ready create', 'a ready create with a TTL', 'a ready create in other forms', 'a ready get', 'a get in other forms', 'a renew with a TTL', 'a renew', 'an expired lease', 'a released lease') { Skip-VcNoCa "lease-api.md#versioning: $n validates against v0's schema and v1's" }
  }

  # --- A pending lease ---
  $d = Test-VcLine -Why 'a pending lease: the host has no size recorded' -Arguments @('lease', 'create', '--project', 'vc-pend') -Kind lease -State pending
  $idp = Get-VcId $d
  if ($idp) {
    [void](Test-VcLine -Why 'a pending lease' -Arguments @('lease', 'get', $idp) -Kind lease -State pending)
    [void](Test-VcLine -Why 'a pending lease, in a list' -Arguments @('lease', 'list', '--project', 'vc-pend', '--state', 'pending') -Kind list)
    [void](Test-VcLine -Why 'a pending lease cannot be renewed' -Arguments @('lease', 'renew', $idp) -Kind error -Code invalid_state -LeaseId $idp -ErrState pending)
    [void](Test-VcLine -Why 'a pending lease released' -Arguments @('lease', 'release', $idp) -Kind lease -State released)
  } else { foreach ($n in 'get', 'list', 'renew', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a pending lease validates against v0's schema and v1's" -Status FAIL -Reason 'The create printed no pending lease that validates, so there was nothing to read back' } }

  # --- An unsatisfiable lease, with each reason v0 had alone and together ---
  $unsat = @(
    @{ Why = 'an unsupported platform'; A = @('--platform', 'windows/amd64'); R = @('unsupported_platform') }
    @{ Why = 'an unsupported platform in the OCI form with a variant'; A = @('--platform=linux/amd64/v3'); R = @('unsupported_platform') }
    @{ Why = 'an unsupported profile'; A = @('--profile', 'agent-linux@1'); R = @('unsupported_profile') }
    @{ Why = 'an unsupported size'; A = @('--size', 'xxl'); R = @('unsupported_size') }
    @{ Why = 'an unsupported size in upper case, with spaces'; A = @('--size', ' XXL '); R = @('unsupported_size') }
    @{ Why = 'an unsupported platform and profile'; A = @('--platform', 'linux/amd64', '--profile', 'agent-linux@2'); R = @('unsupported_platform', 'unsupported_profile') }
    @{ Why = 'an unsupported platform and size'; A = @('--platform', 'linux/amd64', '--size', 'xxl'); R = @('unsupported_platform', 'unsupported_size') }
    @{ Why = 'an unsupported profile and size'; A = @('--profile', 'other', '--size', 'xxl'); R = @('unsupported_profile', 'unsupported_size') }
    @{ Why = 'an unsupported platform, profile and size'; A = @('--platform', 'windows/amd64', '--profile', 'agent-linux@1', '--size', 'xxl', '--ttl', '1h'); R = @('unsupported_platform', 'unsupported_profile', 'unsupported_size') }
  )
  $idu = ''
  foreach ($u in $unsat) {
    $d = Test-VcLine -Why $u.Why -Arguments (@('lease', 'create', '--project', 'vc-ready') + $u.A) -Kind lease -State unsatisfiable -Reasons $u.R
    if (-not $idu) { $idu = Get-VcId $d }
  }
  # no_capacity: a project with no host that is not on the list (D54), and a project whose host serves a smaller lease (D66).
  $d = Test-VcLine -Why 'no capacity: the project has no host and is not on the project list' -Arguments @('lease', 'create', '--project', 'vc-nocap') -Kind lease -State unsatisfiable -Reasons @('no_capacity')
  $idn = Get-VcId $d
  $d = Test-VcLine -Why 'no capacity: with an unsupported platform it is the platform alone' -Arguments @('lease', 'create', '--project', 'vc-nocap', '--platform', 'windows/amd64') -Kind lease -State unsatisfiable -Reasons @('unsupported_platform')
  if ($lab.CaReady) {
    # A live ready lease at m's type holds the host, so a larger size is refused at once with no_capacity (D66).
    $d = Test-VcLine -Why 'a ready lease that holds the host at size m' -Arguments @('lease', 'create', '--project', 'vc-ready', '--size', 'm') -Kind lease -State ready
    $idh = Get-VcId $d
    [void](Test-VcLine -Why 'no capacity: the host serves another lease at a smaller size' -Arguments @('lease', 'create', '--project', 'vc-ready', '--size', 'xl') -Kind lease -State unsatisfiable -Reasons @('no_capacity'))
    if ($idh) { [void](Test-VcLine -Why 'the lease that held the host, released' -Arguments @('lease', 'release', $idh) -Kind lease -State released) }
  } else { Skip-VcNoCa "lease-api.md#versioning: no_capacity for a host that serves a smaller lease validates against v0's schema and v1's" }
  foreach ($pair in @(@('get', $idu), @('get', $idn), @('release', $idu), @('release', $idn), @('release', $idu))) {
    if ($pair[1]) { [void](Test-VcLine -Why "an unsatisfiable lease$(if ($pair[0] -ceq 'release') { ', unchanged' })" -Arguments @('lease', $pair[0], $pair[1]) -Kind lease -State unsatisfiable) }
    else { Write-ConformanceResult -Name "lease-api.md#versioning: $($pair[0]) of an unsatisfiable lease validates against v0's schema and v1's" -Status FAIL -Reason 'No unsatisfiable lease was printed to read back' }
  }
  if ($idu) { [void](Test-VcLine -Why 'an unsatisfiable lease cannot be renewed' -Arguments @('lease', 'renew', $idu) -Kind error -Code invalid_state -LeaseId $idu -ErrState unsatisfiable) }
  [void](Test-VcLine -Why 'every unsatisfiable lease, in a list' -Arguments @('lease', 'list', '--state', 'unsatisfiable') -Kind list)

  # --- Lists ---
  [void](Test-VcLine -Why 'every lease' -Arguments @('lease', 'list') -Kind list)
  [void](Test-VcLine -Why 'one project, in upper case' -Arguments @('lease', 'list', '--project', 'VC-READY') -Kind list)
  [void](Test-VcLine -Why 'two states, in one flag, in other cases' -Arguments @('lease', 'list', '--STATE', 'Ready, Released') -Kind list)
  [void](Test-VcLine -Why 'two states, in two flags' -Arguments @('lease', 'list', '--state', 'pending', '--state=unsatisfiable') -Kind list)
  [void](Test-VcLine -Why 'no lease matches: an empty list' -Arguments @('lease', 'list', '--project', 'nobody-has-this') -Kind list)

  # --- Errors of each code ---
  foreach ($e in @(
      @{ A = @(); F = '' }
      @{ A = @('lease'); F = '' }
      @{ A = @('frob'); F = '' }
      @{ A = @('lease', 'frob', '--help'); F = '' }
      @{ A = @('lease', 'create'); F = 'project' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl'); F = 'ttl' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--size', ''); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--size', 'm', '--size', 'l'); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--bogus'); F = '' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', 'extra'); F = '' }
      @{ A = @('lease', 'get'); F = 'id' }
      @{ A = @('lease', 'get', $notFound, $notFound); F = '' }
      @{ A = @('lease', 'renew', $notFound, '--ttl'); F = 'ttl' }
      @{ A = @('lease', 'release'); F = 'id' }
      @{ A = @('lease', 'list', '--state'); F = 'state' }
      @{ A = @('lease', 'list', 'extra'); F = '' }
    )) {
    [void](Test-VcLine -Why 'a usage error' -Arguments $e.A -Kind error -Code usage -Field $e.F)
  }
  foreach ($e in @(
      @{ A = @('lease', 'create', '--project', 'a_b'); F = 'project' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--platform', 'linux'); F = 'platform' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--profile', 'agent linux'); F = 'profile' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--size', 'toolarge9'); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl', '8'); F = 'ttl' }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl', "8`n"); F = 'ttl' }
      @{ A = @('lease', 'get', 'not-a-uuid'); F = 'id' }
      @{ A = @('lease', 'renew', $notFound, '--ttl', '8'); F = 'ttl' }
      @{ A = @('lease', 'release', 'nope'); F = 'id' }
      @{ A = @('lease', 'list', '--project', '-x-'); F = 'project' }
      @{ A = @('lease', 'list', '--state', 'ready,bogus'); F = 'state' }
    )) {
    [void](Test-VcLine -Why 'a value outside its accepted forms' -Arguments $e.A -Kind error -Code invalid_value -Field $e.F)
  }
  foreach ($e in @(
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl', '48h') }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl', '59s') }
      @{ A = @('lease', 'create', '--project', 'vc-ready', '--ttl=99999999999999999999h') }
      @{ A = @('lease', 'renew', $notFound, '--ttl', 'P2D') }
    )) {
    [void](Test-VcLine -Why 'a TTL out of range' -Arguments $e.A -Kind error -Code out_of_range -Field ttl)
  }
  foreach ($sub in 'get', 'renew', 'release') {
    [void](Test-VcLine -Why 'a lease ID that names no lease' -Arguments @('lease', $sub, $notFound) -Kind error -Code not_found -LeaseId $notFound)
  }
  [void](Test-VcLine -Why 'a lease ID that names no lease, in upper case' -Arguments @('lease', 'get', $notFound.ToUpperInvariant()) -Kind error -Code not_found -LeaseId $notFound)

  # --- internal: no CA, and records that cannot be read ---
  if ($lab.CaReady) {
    $d = Test-VcLine -Why 'a ready lease, made before the CA is removed' -Arguments @('lease', 'create', '--project', 'vc-ready') -Kind lease -State ready
    $idc = Get-VcId $d
    Remove-PvCa -Lab $lab
    [void](Test-VcLine -Why 'no CA: a create that would be ready' -Arguments @('lease', 'create', '--project', 'vc-ready') -Kind error -Code internal)
    if ($idc) {
      [void](Test-VcLine -Why 'no CA: a renew' -Arguments @('lease', 'renew', $idc) -Kind error -Code internal)
      [void](Test-VcLine -Why 'no CA: a release, which needs none' -Arguments @('lease', 'release', $idc) -Kind lease -State released)
    } else { foreach ($n in 'a renew', 'a release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n with no CA validates against v0's schema and v1's" -Status FAIL -Reason 'There was no ready lease to read back' } }
  } else {
    foreach ($n in 'a create', 'a renew', 'a release') { Skip-VcNoCa "lease-api.md#versioning: $n with no CA validates against v0's schema and v1's" }
  }
  # The state file is not a document: every command that reads it fails with internal, and nothing is changed.
  $statePath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.StateDir 'leases.json')
  [IO.File]::WriteAllBytes($statePath, [Text.UTF8Encoding]::new($false).GetBytes("this is not a lease state file`n"))
  [void](Test-VcLine -Why 'the lease records cannot be read: a get' -Arguments @('lease', 'get', $notFound) -Kind error -Code internal)
  [void](Test-VcLine -Why 'the lease records cannot be read: a list' -Arguments @('lease', 'list') -Kind error -Code internal)
  [void](Test-VcLine -Why 'the lease records cannot be read: a create' -Arguments @('lease', 'create', '--project', 'vc-ready', '--platform', 'windows/amd64') -Kind error -Code internal)

  # --- Controls: what v1 adds MUST fail v0's schema, so the checks above are not vacuous ---
  $plansDir = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.Root 'plans')
  New-Item -ItemType Directory -Force -Path $plansDir | Out-Null
  $okPlan = '{"name":"vc-plan","turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}'
  $stories = @(1..32 | ForEach-Object { '{"name":"s' + $_ + '","subagents":[{"name":"A","model":"Opus","effort":"high"}]}' }) -join ','
  $bigPlan = '{"name":"vc-big","turns":[{"name":"T1","stories":[' + $stories + ']}]}'
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $plansDir 'ok.json')), [Text.UTF8Encoding]::new($false).GetBytes($okPlan))
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $plansDir 'big.json')), [Text.UTF8Encoding]::new($false).GetBytes($bigPlan))
  # The state file is put right again, so the lease the controls make is recorded.
  Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $statePath) -Force
  $okPath = Join-Path $plansDir 'ok.json'
  $bigPath = Join-Path $plansDir 'big.json'
  $controls = @(
    @{ Why = 'a lease made with --plan holds sized_for, which v0 never printed'; A = @('lease', 'create', '--project', 'vc-nocap', '--plan', $okPath); Kind = 'lease' }
    @{ Why = 'a lease whose plan no size holds has the reason plan_too_large and sized_for, which v0 never printed'; A = @('lease', 'create', '--project', 'vc-nocap', '--plan', $bigPath); Kind = 'lease' }
    @{ Why = '--plan with --size is a usage error for the field plan, which v0 never named'; A = @('lease', 'create', '--project', 'vc-nocap', '--plan', $okPath, '--size', 'm'); Kind = 'error' }
    @{ Why = 'a plan that is not a file is invalid_value for the field plan, which v0 never named'; A = @('lease', 'create', '--project', 'vc-nocap', '--plan', ($okPath + '.missing')); Kind = 'error' }
  )
  foreach ($c in $controls) {
    $name = "lease-api.md#versioning: Control: $($c.Why), so it validates against v1's schema and not v0's"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $run = Invoke-VcCli -Arguments $c.A
      $schemas = Get-VcSchemas $c.Kind
      $text = if ($c.Kind -ceq 'error') { [string]$run.Stderr } else { [string]$run.Stdout }
      $want = if ($c.Kind -ceq 'error') { 2 } else { 0 }
      if ($run.ExitCode -ne $want) { $p.Add("Exit $($run.ExitCode), not $want") }
      if ([string]::IsNullOrWhiteSpace($text)) { $p.Add('There is no document') }
      else {
        if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas.V1)) { $p.Add("The document does not validate against $($schemas.V1)") }
        if (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas.V0) { $p.Add("The document validates against $($schemas.V0), which has no place for what v1 added") }
      }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-VcCheck -Name $name -Problems $p
  }

  # --- Every state, reason and error code was seen, and enough documents were made ---
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($s in 'ready', 'pending', 'expired', 'released', 'unsatisfiable') { if (-not $script:VcStates.ContainsKey($s)) { $p.Add("No lease in the state $s was validated") } }
  foreach ($r in 'unsupported_platform', 'unsupported_profile', 'unsupported_size', 'no_capacity') { if (-not $script:VcReasons.ContainsKey($r)) { $p.Add("No lease with the reason $r was validated") } }
  foreach ($c in 'usage', 'invalid_value', 'out_of_range', 'not_found', 'invalid_state', 'internal') { if (-not $script:VcCodes.ContainsKey($c)) { $p.Add("No error with the code $c was validated") } }
  if ($script:VcDocs -lt 80) { $p.Add("Only $($script:VcDocs) documents were validated, not at least 80") }
  if ($p.Count -gt 0 -and -not $lab.CaReady) { Write-ConformanceResult -Name "lease-api.md#versioning: Every state, reason and error code of v0 was printed and validated" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)" }
  else { Complete-VcCheck -Name 'lease-api.md#versioning: Every state, every reason and every error code of v0 was printed and validated against both versions, in at least 80 documents' -Problems $p }
} catch {
  Write-ConformanceResult -Name 'test-v1-compatible.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(Format-VcText $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) {
    try { Clear-PvCleanup -Lab $lab } catch { }
    Remove-ConformanceHome -ConformanceHome $lab
  }
}
Exit-ConformanceRun

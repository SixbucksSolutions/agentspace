#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.11-T04-S01, #0239): contract v2 keeps contract v1, and v0 where v1 kept it
# (docs/lease-api.md, Versioning, "What v2 keeps of v1", and Schemas; D76, D87). Every document that a command line
# with none of `--board`, `--epic` and `--turn`, and with no `budget_minutes` in its plan, prints, on stdout or on
# stderr, validates against v1's schema in `schemas/v1/` as well as against v2's at the top of `schemas/`, and one
# with no `--plan` against v0's in `schemas/v0/` too: a lease in each state it can be in (ready, pending, expired,
# released, unsatisfiable), each reason a lease can have (unsupported_platform, unsupported_profile,
# unsupported_size, no_capacity, plan_too_large), a list, and an error of each code (usage, invalid_value,
# out_of_range, not_found, invalid_state, internal). It drives the lease CLI only as a separate process, through the
# harness, in a throwaway home, with a stand-in provisioning run and a host registry the check seeds, as every other
# lease CLI check does. Every expectation is taken from docs/lease-api.md, never from a program's output. It starts no
# board, and reads none: no line below gives `--board` with a value that names a board it could reach, and every line
# with `--board` is refused before any request is made.
#
# Groups:
#   Plan-less    The lines of test-v1-compatible.ps1, so v2 shows it keeps what v1 kept of v0: validated against v2's,
#                v1's and v0's schemas. A lease MUST NOT hold `sized_for` or `plan_too_large`, and an error MUST NOT
#                name the field `plan`, `board`, `epic` or `turn`, since none of those was given.
#   With a plan  Lines with `--plan` and no `--turn`, whose plans have no `budget_minutes`: validated against v2's and
#                v1's schemas, and not v0's, which has no place for `sized_for` and the field `plan`. A plan with no
#                budget gives the lease v1 gave it: the TTL of 8 hours (lease-api.md, TTL).
#   v2's inputs  Lines that only v2 accepts, with `--turn` or a plan with `budget_minutes`: the lease each prints is a
#                document v1's schemas validate too, "whatever made the lease" (lease-api.md, Versioning, "What is
#                not a change"). Validated against v2's and v1's.
#   Controls     So that a validator that accepted everything cannot pass this file: a lease made with `--plan`, a
#                `plan_too_large` lease and the errors that name the field `plan` validate against v2's and v1's
#                schemas and not v0's; and every error that names the field `board`, `epic` or `turn`, which v1 has
#                no place for, validates against v2's error schema and fails v1's and v0's (lease-api.md, Schemas).
#
# A result at the end shows that every state, every reason and every error code above was seen, and enough documents
# were made, so a run that made too few cannot pass.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home, and every create here
# is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it is given and makes no host.
# Every write calls Assert-ConformancePath at the call. The program is run with the plans' folder as its working
# folder, so an argument is a bare file name and no check name or message holds a path of the machine's. No program of
# agentspace's but the lease CLI is started, and only through the harness. A failure reason is masked before it is
# printed, and never quotes a value the stand-in captured. Nothing here reads Terry's receipt, credential or board, and
# nothing connects to a port.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-v2-compatible.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped (no CA, for the ready cases).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

$script:VkLab = $null
$script:VkStandIn = $null
$script:VkStates = @{}
$script:VkReasons = @{}
$script:VkCodes = @{}
$script:VkDocs = 0
$script:VkUuid = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'
# The most agents at once that xl holds at today's footprint (lease-api.md, Plan), the case's own constant.
$script:VkMost = 32

function Format-VkText([string]$Text) {
  $x = [string]$Text
  $x = $x -replace '[A-Za-z]:[\\/][^\s''"]*', '<path>'
  $x = $x -replace '\s+', ' '
  if ($x.Length -gt 260) { $x = $x.Substring(0, 260) + '...' }
  return $x
}

function Complete-VkCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ((@($Problems | ForEach-Object { Format-VkText $_ })) -join '; ') }
}

# The names of this process's variables that start LOCALSWIM_, found by listing names alone and never reading a value,
# each removed from every run: a line here gives `--board` and is refused before any read, and a build that wrongly read
# one would otherwise reach a real board (docs/conformance.md, "The stand-in credential").
$script:VkLocalswimNames = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -like 'LOCALSWIM_*' })

# `agentspace <Arguments>` as typed (the first word is `lease`), with the check's ProxyCommand, the stand-in as the
# provisioning run, and no LOCALSWIM_* variable.
function Invoke-VkCli {
  param([Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments)
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy; AGENTSPACE_PROVISION_CMD = $script:VkStandIn.Path }
  foreach ($n in $script:VkLocalswimNames) { $e[$n] = $null }
  return (Invoke-AgentspaceCli -ConformanceHome $script:VkLab -Arguments $Arguments -Environment $e)
}

# The schemas of a kind of document, each by its path under schemas/: v2's at the top, v1's under v1/, v0's under v0/.
function Get-VkSchemas([string]$Kind) {
  switch ($Kind) {
    'lease' { return @{ V2 = 'lease.output.schema.json'; V1 = 'v1/lease.output.schema.json'; V0 = 'v0/lease.output.schema.json' } }
    'list' { return @{ V2 = 'lease-list.output.schema.json'; V1 = 'v1/lease-list.output.schema.json'; V0 = 'v0/lease-list.output.schema.json' } }
    'error' { return @{ V2 = 'error.output.schema.json'; V1 = 'v1/error.output.schema.json'; V0 = 'v0/error.output.schema.json' } }
  }
}

# The problems with a run's document, which MUST validate against v2's schema and v1's, and v0's unless the line has a
# plan. A success is judged as lease-api.md, Output, says; a failure as Exit codes and errors says. -Plan marks a line
# that gives --plan (or whose plan has a budget): such a document MAY hold what v1 added, and v0 has no place for it.
# A line with no plan gets none of it, and no field that names board, epic or turn.
function Get-VkProblems {
  param(
    [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][ValidateSet('lease', 'list', 'error')][string]$Kind, [switch]$Plan,
    [string]$Code, [string]$Field = '', [string]$State, [string]$LeaseId = '', [string]$ErrState = '', [string[]]$Reasons
  )
  $schemas = Get-VkSchemas $Kind
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Kind -ceq 'error') {
    foreach ($x in (Get-ConformanceErrorProblems -Run $Run -Code $Code -Field $Field -LeaseId $LeaseId -State $ErrState)) { $p.Add($x) }
    $text = [string]$Run.Stderr
    $doc = $Run.ErrorJson
  } else {
    foreach ($x in (Get-ConformanceSuccessProblems -Run $Run -Schema $schemas.V2)) { $p.Add($x) }
    $text = [string]$Run.Stdout
    $doc = $Run.Json
  }
  if ($null -eq $doc) { return , $p }
  $script:VkDocs++
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas.V1)) { $p.Add("The document does not validate against $($schemas.V1), contract v1's schema") }
  if (-not $Plan -and -not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas.V0)) { $p.Add("The document does not validate against $($schemas.V0), contract v0's schema") }
  $leases = @()
  if ($Kind -ceq 'lease') { $leases = @($doc) }
  elseif ($Kind -ceq 'list') { $leases = @($doc['leases']) }
  foreach ($l in $leases) {
    if ($l -isnot [hashtable]) { continue }
    if (-not $Plan -and $l.ContainsKey('sized_for')) { $p.Add('A lease made with no plan holds sized_for') }
    $script:VkStates[[string]$l['state']] = $true
    foreach ($r in @($l['reasons'] | Where-Object { $null -ne $_ })) {
      $script:VkReasons[[string]$r['code']] = $true
      if (-not $Plan -and $r['code'] -ceq 'plan_too_large') { $p.Add('A lease made with no plan has the reason plan_too_large') }
    }
  }
  if ($Kind -ceq 'lease' -and $State -and $doc['state'] -cne $State) { $p.Add("The state is '$($doc['state'])', not '$State'") }
  if ($Kind -ceq 'lease' -and $Reasons) {
    $got = @($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['code'] })
    if (($got -join ',') -cne ($Reasons -join ',')) { $p.Add("The reasons are [$($got -join ', ')], not [$($Reasons -join ', ')]") }
  }
  if ($Kind -ceq 'error') {
    $script:VkCodes[[string]$doc['code']] = $true
    if ($doc.ContainsKey('field')) {
      if (-not $Plan -and $doc['field'] -ceq 'plan') { $p.Add("An error for a line with no plan names the field 'plan'") }
      if ($doc['field'] -cin 'board', 'epic', 'turn') { $p.Add("An error for a line that gives none of --board, --epic and --turn names the field '$($doc['field'])'") }
    }
  }
  return , $p
}

# Runs a line and checks its document against the schemas. Returns the parsed lease or list, or $null when the document
# did not pass, so a check that goes on from it never builds on a document it has not validated.
function Test-VkLine {
  param(
    [Parameter(Mandatory)][string]$Why, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [Parameter(Mandatory)][ValidateSet('lease', 'list', 'error')][string]$Kind,
    [switch]$Plan, [string]$Code, [string]$Field = '', [string]$State, [string]$LeaseId = '', [string]$ErrState = '', [string[]]$Reasons, [int]$Ttl = 0
  )
  $what = switch ($Kind) {
    'lease' { "a lease$(if ($State) { ", $State" })$(if ($Reasons) { ", reasons $($Reasons -join ' then ')" })" }
    'list' { 'a list' }
    'error' { "a $Code error$(if ($Field) { " for '$Field'" })" }
  }
  $against = if ($Plan) { "v2's schema and v1's" } else { "v0's schema, v1's and v2's" }
  $name = "lease-api.md#versioning: $(Format-ConformanceLine $Arguments) prints $what ($Why) that validates against $against"
  $p = [System.Collections.Generic.List[string]]::new()
  $result = $null
  try {
    $run = Invoke-VkCli -Arguments $Arguments
    $splat = @{ Run = $run; Kind = $Kind; Field = $Field; LeaseId = $LeaseId; ErrState = $ErrState; Plan = [bool]$Plan }
    if ($Code) { $splat['Code'] = $Code }
    if ($State) { $splat['State'] = $State }
    if ($Reasons) { $splat['Reasons'] = $Reasons }
    foreach ($x in (Get-VkProblems @splat)) { $p.Add($x) }
    if ($p.Count -eq 0 -and $Ttl -gt 0 -and $Kind -ceq 'lease' -and $run.Json['ttl_seconds'] -ne $Ttl) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not $Ttl") }
    if ($p.Count -eq 0) { $result = if ($Kind -ceq 'error') { $run.ErrorJson } else { $run.Json } }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-VkCheck -Name $name -Problems $p
  return $result
}

function Skip-VkNoCa([string]$Name) {
  Write-ConformanceResult -Name $Name -Status SKIPPED -Reason "No throwaway CA could be made: $($script:VkLab.CaReason)"
}

# A lease ID from a document that passed its schema, or '' (so no later line is built on an ID that was never seen).
function Get-VkId($Doc) {
  if ($Doc -is [hashtable] -and [string]$Doc['id'] -cmatch $script:VkUuid) { return [string]$Doc['id'] }
  return ''
}

# Writes a plan file in the plans folder of the home and returns its bare name, which is the program's working folder.
function Write-VkPlan {
  param([Parameter(Mandatory)][string]$File, [Parameter(Mandatory)][string]$Text)
  if (-not (Test-ConformanceSchema -Json $Text -Schema 'plan.input.schema.json')) { throw "The case wrote a plan, $File, that does not validate against its schema." }
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:VkLab -Path (Join-Path $script:VkDir $File)), [Text.UTF8Encoding]::new($false).GetBytes($Text))
  return $File
}

function New-VkSub([string]$Name) { '{"name":"' + $Name + '","model":"Opus","effort":"high"}' }
# A turn of $Stories stories of one subagent each: its agents at once are $Stories + 1.
function New-VkTurn {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Stories, [int]$Budget = 0)
  $st = @(1..$Stories | ForEach-Object { '{"name":"s' + $_ + '","subagents":[' + (New-VkSub 'a') + ']}' }) -join ','
  $b = if ($Budget -gt 0) { ',"budget_minutes":' + $Budget } else { '' }
  return '{"name":"' + $Name + '"' + $b + ',"stories":[' + $st + ']}'
}

$lab = $null
$savedCwd = [Environment]::CurrentDirectory
try {
  $lab = New-ConformanceHome
  $script:VkLab = $lab
  $script:VkStandIn = New-PvStandIn -Lab $lab -Name 'v2-compatible'
  $script:VkDir = Join-Path $lab.Root 'plans'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $lab -Path $script:VkDir) | Out-Null
  [Environment]::CurrentDirectory = (Assert-ConformancePath -ConformanceHome $lab -Path $script:VkDir)
  $notFound = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'
  $boardId = '0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f'
  # v2c-ready has a ready host at m's type, so a default create is ready; v2c-pend has a ready host whose size is not
  # recorded, which answers pending (lease-api.md, "A host's life follows its leases"); v2c-nocap has no host and is not
  # on the project list, so it answers no_capacity (D54).
  [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project 'v2c-ready')
  [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project 'v2c-pend' -NoInstanceType)

  # Plans, none of which has a budget_minutes except the ones of the last group.
  $okPlan = Write-VkPlan -File 'ok.json' -Text ('{"name":"v2c-plan","turns":[' + (New-VkTurn -Name 'T1' -Stories 1) + ']}')
  $midPlan = Write-VkPlan -File 'mid.json' -Text ('{"name":"v2c-mid","turns":[' + (New-VkTurn -Name 'T1' -Stories 2) + ',' + (New-VkTurn -Name 'T2' -Stories 5) + ']}')
  $bigPlan = Write-VkPlan -File 'big.json' -Text ('{"name":"v2c-big","turns":[' + (New-VkTurn -Name 'T1' -Stories 32) + ']}')
  $nonamePlan = Write-VkPlan -File 'noname.json' -Text ('{"turns":[' + (New-VkTurn -Name 'T1' -Stories 2) + ']}')
  # With budgets: T1 of 3 agents at once and 45 minutes, T2 of 6 and 120 minutes (the busiest); and, for the too-large cases, T1 of 3 and 45 and T2 of 33 and 30.
  $budgetPlan = Write-VkPlan -File 'budget.json' -Text ('{"name":"v2c-budget","turns":[' + (New-VkTurn -Name 'T1' -Stories 4 -Budget 45) + ',' + (New-VkTurn -Name 'T2' -Stories 5 -Budget 120) + ']}')
  $budgetBigPlan = Write-VkPlan -File 'budget-big.json' -Text ('{"name":"v2c-budget-big","turns":[' + (New-VkTurn -Name 'T1' -Stories 2 -Budget 45) + ',' + (New-VkTurn -Name 'T2' -Stories 32 -Budget 30) + ']}')

  # ===== Plan-less lines: v2 keeps what v1 kept of v0 =====
  # --- A ready lease, and everything a command does with it ---
  $id1 = ''
  $id2 = ''
  if ($lab.CaReady) {
    $d = Test-VkLine -Why 'a ready lease' -Arguments @('lease', 'create', '--project', 'v2c-ready') -Kind lease -State ready -Ttl 28800
    $id1 = Get-VkId $d
    $d = Test-VkLine -Why 'a ready lease with a TTL' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--ttl', '90m') -Kind lease -State ready -Ttl 5400
    $id2 = Get-VkId $d
    $d = Test-VkLine -Why 'every flag but a plan, in other cases and forms' -Arguments @('LEASE', 'Create', '--PROJECT=V2C-READY', '--Platform', ' Linux/AArch64 ', '--profile=Agent-Linux', '--Size', 'M', '--TTL=1H') -Kind lease -State ready -Ttl 3600
    $id3 = Get-VkId $d
    if ($id1) {
      [void](Test-VkLine -Why 'a ready lease' -Arguments @('lease', 'get', $id1) -Kind lease -State ready)
      [void](Test-VkLine -Why 'a ready lease, the ID in upper case with spaces' -Arguments @('Lease', 'GET', "  $($id1.ToUpperInvariant())  ") -Kind lease -State ready)
      [void](Test-VkLine -Why 'a ready lease renewed with a TTL' -Arguments @('lease', 'renew', $id1, '--ttl', '2h') -Kind lease -State ready -Ttl 7200)
      [void](Test-VkLine -Why 'a ready lease renewed with its own TTL' -Arguments @('lease', 'renew', $id1) -Kind lease -State ready -Ttl 7200)
    } else { foreach ($n in 'get', 'get in other forms', 'renew with a TTL', 'renew') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a ready lease validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'The create printed no ready lease that validates, so there was nothing to read back' } }
    # An expired lease: a ready lease whose expires_at has passed (state.md, the lease state file), read by each command.
    if ($id2 -and (Set-ConformanceRecordMember -ConformanceHome $lab -Id $id2 -Member 'expires_at' -Value '2020-01-01T00:00:00Z')) {
      [void](Test-VkLine -Why 'an expired lease' -Arguments @('lease', 'get', $id2) -Kind lease -State expired)
      [void](Test-VkLine -Why 'an expired lease, in a list' -Arguments @('lease', 'list', '--project', 'v2c-ready', '--state', 'expired') -Kind list)
      [void](Test-VkLine -Why 'an expired lease cannot be renewed' -Arguments @('lease', 'renew', $id2) -Kind error -Code invalid_state -LeaseId $id2 -ErrState expired)
      [void](Test-VkLine -Why 'an expired lease released' -Arguments @('lease', 'release', $id2) -Kind lease -State released)
    } else { foreach ($n in 'get', 'list', 'renew', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of an expired lease validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'There was no ready lease to make expired' } }
    # A released lease, released again.
    if ($id1) {
      [void](Test-VkLine -Why 'a ready lease released' -Arguments @('lease', 'release', $id1) -Kind lease -State released)
      [void](Test-VkLine -Why 'a released lease released again' -Arguments @('lease', 'release', $id1) -Kind lease -State released)
      [void](Test-VkLine -Why 'a released lease' -Arguments @('lease', 'get', $id1) -Kind lease -State released)
      [void](Test-VkLine -Why 'a released lease cannot be renewed' -Arguments @('lease', 'renew', $id1) -Kind error -Code invalid_state -LeaseId $id1 -ErrState released)
    } else { foreach ($n in 'release', 'a second release', 'get', 'renew') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a released lease validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'There was no ready lease to release' } }
    if ($id3) { [void](Test-VkLine -Why 'a lease made in other cases and forms, released' -Arguments @('lease', 'release', $id3) -Kind lease -State released) }
  } else {
    foreach ($n in 'a ready create', 'a ready create with a TTL', 'a ready create in other forms', 'a ready get', 'a get in other forms', 'a renew with a TTL', 'a renew', 'an expired lease', 'a released lease') { Skip-VkNoCa "lease-api.md#versioning: $n validates against v0's schema, v1's and v2's" }
  }

  # --- A pending lease ---
  $d = Test-VkLine -Why 'a pending lease: the host has no size recorded' -Arguments @('lease', 'create', '--project', 'v2c-pend') -Kind lease -State pending
  $idp = Get-VkId $d
  if ($idp) {
    [void](Test-VkLine -Why 'a pending lease' -Arguments @('lease', 'get', $idp) -Kind lease -State pending)
    [void](Test-VkLine -Why 'a pending lease, in a list' -Arguments @('lease', 'list', '--project', 'v2c-pend', '--state', 'pending') -Kind list)
    [void](Test-VkLine -Why 'a pending lease cannot be renewed' -Arguments @('lease', 'renew', $idp) -Kind error -Code invalid_state -LeaseId $idp -ErrState pending)
    [void](Test-VkLine -Why 'a pending lease released' -Arguments @('lease', 'release', $idp) -Kind lease -State released)
  } else { foreach ($n in 'get', 'list', 'renew', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a pending lease validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'The create printed no pending lease that validates, so there was nothing to read back' } }

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
    $d = Test-VkLine -Why $u.Why -Arguments (@('lease', 'create', '--project', 'v2c-ready') + $u.A) -Kind lease -State unsatisfiable -Reasons $u.R
    if (-not $idu) { $idu = Get-VkId $d }
  }
  # no_capacity: a project with no host that is not on the list (D54), and a project whose host serves a smaller lease (D66).
  $d = Test-VkLine -Why 'no capacity: the project has no host and is not on the project list' -Arguments @('lease', 'create', '--project', 'v2c-nocap') -Kind lease -State unsatisfiable -Reasons @('no_capacity')
  $idn = Get-VkId $d
  [void](Test-VkLine -Why 'no capacity: with an unsupported platform it is the platform alone' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--platform', 'windows/amd64') -Kind lease -State unsatisfiable -Reasons @('unsupported_platform'))
  if ($lab.CaReady) {
    # A live ready lease at m's type holds the host, so a larger size is refused at once with no_capacity (D66).
    $d = Test-VkLine -Why 'a ready lease that holds the host at size m' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--size', 'm') -Kind lease -State ready
    $idh = Get-VkId $d
    [void](Test-VkLine -Why 'no capacity: the host serves another lease at a smaller size' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--size', 'xl') -Kind lease -State unsatisfiable -Reasons @('no_capacity'))
    if ($idh) { [void](Test-VkLine -Why 'the lease that held the host, released' -Arguments @('lease', 'release', $idh) -Kind lease -State released) }
  } else { Skip-VkNoCa "lease-api.md#versioning: no_capacity for a host that serves a smaller lease validates against v0's schema, v1's and v2's" }
  foreach ($pair in @(@('get', $idu), @('get', $idn), @('release', $idu), @('release', $idn), @('release', $idu))) {
    if ($pair[1]) { [void](Test-VkLine -Why "an unsatisfiable lease$(if ($pair[0] -ceq 'release') { ', unchanged' })" -Arguments @('lease', $pair[0], $pair[1]) -Kind lease -State unsatisfiable) }
    else { Write-ConformanceResult -Name "lease-api.md#versioning: $($pair[0]) of an unsatisfiable lease validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'No unsatisfiable lease was printed to read back' }
  }
  if ($idu) { [void](Test-VkLine -Why 'an unsatisfiable lease cannot be renewed' -Arguments @('lease', 'renew', $idu) -Kind error -Code invalid_state -LeaseId $idu -ErrState unsatisfiable) }
  [void](Test-VkLine -Why 'every unsatisfiable lease, in a list' -Arguments @('lease', 'list', '--state', 'unsatisfiable') -Kind list)

  # --- Lists ---
  [void](Test-VkLine -Why 'every lease' -Arguments @('lease', 'list') -Kind list)
  [void](Test-VkLine -Why 'one project, in upper case' -Arguments @('lease', 'list', '--project', 'V2C-READY') -Kind list)
  [void](Test-VkLine -Why 'two states, in one flag, in other cases' -Arguments @('lease', 'list', '--STATE', 'Ready, Released') -Kind list)
  [void](Test-VkLine -Why 'two states, in two flags' -Arguments @('lease', 'list', '--state', 'pending', '--state=unsatisfiable') -Kind list)
  [void](Test-VkLine -Why 'no lease matches: an empty list' -Arguments @('lease', 'list', '--project', 'nobody-has-this') -Kind list)

  # --- Errors of each code ---
  foreach ($e in @(
      @{ A = @(); F = '' }
      @{ A = @('lease'); F = '' }
      @{ A = @('frob'); F = '' }
      @{ A = @('lease', 'frob', '--help'); F = '' }
      @{ A = @('lease', 'create'); F = 'project' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl'); F = 'ttl' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--size', ''); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--size', 'm', '--size', 'l'); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--bogus'); F = '' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', 'extra'); F = '' }
      @{ A = @('lease', 'get'); F = 'id' }
      @{ A = @('lease', 'get', $notFound, $notFound); F = '' }
      @{ A = @('lease', 'renew', $notFound, '--ttl'); F = 'ttl' }
      @{ A = @('lease', 'release'); F = 'id' }
      @{ A = @('lease', 'list', '--state'); F = 'state' }
      @{ A = @('lease', 'list', 'extra'); F = '' }
    )) {
    [void](Test-VkLine -Why 'a usage error' -Arguments $e.A -Kind error -Code usage -Field $e.F)
  }
  foreach ($e in @(
      @{ A = @('lease', 'create', '--project', 'a_b'); F = 'project' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--platform', 'linux'); F = 'platform' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--profile', 'agent linux'); F = 'profile' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--size', 'toolarge9'); F = 'size' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl', '8'); F = 'ttl' }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl', "8`n"); F = 'ttl' }
      @{ A = @('lease', 'get', 'not-a-uuid'); F = 'id' }
      @{ A = @('lease', 'renew', $notFound, '--ttl', '8'); F = 'ttl' }
      @{ A = @('lease', 'release', 'nope'); F = 'id' }
      @{ A = @('lease', 'list', '--project', '-x-'); F = 'project' }
      @{ A = @('lease', 'list', '--state', 'ready,bogus'); F = 'state' }
    )) {
    [void](Test-VkLine -Why 'a value outside its accepted forms' -Arguments $e.A -Kind error -Code invalid_value -Field $e.F)
  }
  foreach ($e in @(
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl', '48h') }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl', '59s') }
      @{ A = @('lease', 'create', '--project', 'v2c-ready', '--ttl=99999999999999999999h') }
      @{ A = @('lease', 'renew', $notFound, '--ttl', 'P2D') }
    )) {
    [void](Test-VkLine -Why 'a TTL out of range' -Arguments $e.A -Kind error -Code out_of_range -Field ttl)
  }
  foreach ($sub in 'get', 'renew', 'release') {
    [void](Test-VkLine -Why 'a lease ID that names no lease' -Arguments @('lease', $sub, $notFound) -Kind error -Code not_found -LeaseId $notFound)
  }
  [void](Test-VkLine -Why 'a lease ID that names no lease, in upper case' -Arguments @('lease', 'get', $notFound.ToUpperInvariant()) -Kind error -Code not_found -LeaseId $notFound)

  # ===== Lines with --plan, no --turn and no budget_minutes: v2 keeps what v1 printed =====
  # A plan with no budget gives the TTL of 8 hours (lease-api.md, TTL, "When that turn has none").
  [void](Test-VkLine -Plan -Why 'a plan with no budget: the TTL is the default' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $midPlan) -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 28800)
  [void](Test-VkLine -Plan -Why 'a plan with no name: sized_for has no plan' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $nonamePlan) -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 28800)
  [void](Test-VkLine -Plan -Why 'a plan with an unsupported platform' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $midPlan, '--platform', 'windows/amd64') -Kind lease -State unsatisfiable -Reasons @('unsupported_platform') -Ttl 28800)
  [void](Test-VkLine -Plan -Why 'a plan with a TTL' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $midPlan, '--ttl', '2h') -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 7200)
  $d = Test-VkLine -Plan -Why 'a plan no size holds' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $bigPlan) -Kind lease -State unsatisfiable -Reasons @('plan_too_large') -Ttl 28800
  $idt = Get-VkId $d
  [void](Test-VkLine -Plan -Why 'a plan no size holds, with a platform no backend has' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $bigPlan, '--platform', 'windows/amd64') -Kind lease -State unsatisfiable -Reasons @('plan_too_large'))
  if ($idt) {
    [void](Test-VkLine -Plan -Why 'a plan no size holds, read back' -Arguments @('lease', 'get', $idt) -Kind lease -State unsatisfiable -Reasons @('plan_too_large'))
    [void](Test-VkLine -Plan -Why 'a plan no size holds, released' -Arguments @('lease', 'release', $idt) -Kind lease -State unsatisfiable -Reasons @('plan_too_large'))
  } else { foreach ($n in 'get', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a plan_too_large lease validates against v2's schema and v1's" -Status FAIL -Reason 'The create printed no plan_too_large lease that validates, so there was nothing to read back' } }
  $d = Test-VkLine -Plan -Why 'a plan on a project whose host has no size recorded' -Arguments @('lease', 'create', '--project', 'v2c-pend', '--plan', $midPlan) -Kind lease -State pending -Ttl 28800
  $idpp = Get-VkId $d
  if ($idpp) {
    [void](Test-VkLine -Plan -Why 'a pending plan lease' -Arguments @('lease', 'get', $idpp) -Kind lease -State pending)
    [void](Test-VkLine -Plan -Why 'a pending plan lease released' -Arguments @('lease', 'release', $idpp) -Kind lease -State released)
  }
  if ($lab.CaReady) {
    $d = Test-VkLine -Plan -Why 'a plan on a project with a ready host' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $midPlan) -Kind lease -State ready -Ttl 28800
    $idr = Get-VkId $d
    if ($idr) {
      [void](Test-VkLine -Plan -Why 'a ready plan lease' -Arguments @('lease', 'get', $idr) -Kind lease -State ready)
      [void](Test-VkLine -Plan -Why 'a ready plan lease renewed with its own TTL' -Arguments @('lease', 'renew', $idr) -Kind lease -State ready -Ttl 28800)
      [void](Test-VkLine -Plan -Why 'a ready plan lease renewed with a TTL' -Arguments @('lease', 'renew', $idr, '--ttl', '45m') -Kind lease -State ready -Ttl 2700)
      if (Set-ConformanceRecordMember -ConformanceHome $lab -Id $idr -Member 'expires_at' -Value '2020-01-01T00:00:00Z') {
        [void](Test-VkLine -Plan -Why 'an expired plan lease' -Arguments @('lease', 'get', $idr) -Kind lease -State expired)
        [void](Test-VkLine -Plan -Why 'an expired plan lease cannot be renewed' -Arguments @('lease', 'renew', $idr) -Kind error -Code invalid_state -LeaseId $idr -ErrState expired)
      } else { Write-ConformanceResult -Name "lease-api.md#versioning: an expired plan lease validates against v2's schema and v1's" -Status FAIL -Reason 'There was no ready plan lease to make expired' }
      [void](Test-VkLine -Plan -Why 'a plan lease released' -Arguments @('lease', 'release', $idr) -Kind lease -State released)
      [void](Test-VkLine -Plan -Why 'a released plan lease' -Arguments @('lease', 'get', $idr) -Kind lease -State released)
    } else { foreach ($n in 'get', 'renew', 'an expired lease', 'release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n of a ready plan lease validates against v2's schema and v1's" -Status FAIL -Reason 'The create printed no ready plan lease that validates, so there was nothing to read back' } }
  } else { Skip-VkNoCa "lease-api.md#versioning: a ready plan lease and what a command does with it validate against v2's schema and v1's" }
  [void](Test-VkLine -Plan -Why 'every plan lease, in a list' -Arguments @('lease', 'list', '--project', 'v2c-nocap') -Kind list)
  # Errors of a line with a plan: the field plan is v1's.
  [void](Test-VkLine -Plan -Why '--plan with --size' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $okPlan, '--size', 'm') -Kind error -Code usage -Field plan)
  [void](Test-VkLine -Plan -Why '--size with --plan' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--size', 'm', '--plan', $okPlan) -Kind error -Code usage -Field plan)
  [void](Test-VkLine -Plan -Why '--plan with no value' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan') -Kind error -Code usage -Field plan)
  [void](Test-VkLine -Plan -Why '--plan given twice' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $okPlan, '--plan', $okPlan) -Kind error -Code usage -Field plan)
  [void](Test-VkLine -Plan -Why 'a plan that is no file' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', 'missing.json') -Kind error -Code invalid_value -Field plan)
  [void](Test-VkLine -Plan -Why 'a plan that is a folder' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', '.') -Kind error -Code invalid_value -Field plan)
  $null = Write-VkPlan -File 'two-turns.json' -Text ('{"turns":[' + (New-VkTurn -Name 'T1' -Stories 1) + ',' + (New-VkTurn -Name 'T1' -Stories 1) + ']}')
  [void](Test-VkLine -Plan -Why 'a turn named twice' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', 'two-turns.json') -Kind error -Code invalid_value -Field plan)
  [void](Test-VkLine -Plan -Why 'a plan and a TTL that has no unit' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $okPlan, '--ttl', '8') -Kind error -Code invalid_value -Field ttl)
  [void](Test-VkLine -Plan -Why 'a plan and a TTL out of range' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $okPlan, '--ttl', '48h') -Kind error -Code out_of_range -Field ttl)
  [void](Test-VkLine -Plan -Why 'a plan and a project outside its form' -Arguments @('lease', 'create', '--project', 'a_b', '--plan', $okPlan) -Kind error -Code invalid_value -Field project)

  # ===== v2's own inputs print documents that v1's schemas validate too =====
  $idb = ''
  [void](Test-VkLine -Plan -Why 'a plan with budgets and no --turn: the busiest turn T2 of 120 minutes' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $budgetPlan) -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 10800)
  [void](Test-VkLine -Plan -Why 'a plan with budgets and --turn T1 of 45 minutes' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $budgetPlan, '--turn', 'T1') -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 4080)
  [void](Test-VkLine -Plan -Why '--turn with --ttl: the TTL wins' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $budgetPlan, '--turn', 'T1', '--ttl', '30m') -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 1800)
  [void](Test-VkLine -Plan -Why '--turn on a plan with no budgets' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $midPlan, '--turn', 'T1') -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 28800)
  [void](Test-VkLine -Plan -Why '--turn naming a turn that no size holds' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $budgetBigPlan, '--turn', 'T2') -Kind lease -State unsatisfiable -Reasons @('plan_too_large') -Ttl 2700)
  [void](Test-VkLine -Plan -Why '--turn naming a turn that fits, though another turn is too large' -Arguments @('lease', 'create', '--project', 'v2c-nocap', '--plan', $budgetBigPlan, '--turn', 'T1') -Kind lease -State unsatisfiable -Reasons @('no_capacity') -Ttl 4080)
  [void](Test-VkLine -Plan -Why 'a plan with budgets and an unsupported platform' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $budgetPlan, '--turn', 'T2', '--platform', 'windows/amd64') -Kind lease -State unsatisfiable -Reasons @('unsupported_platform') -Ttl 10800)
  if ($lab.CaReady) {
    $d = Test-VkLine -Plan -Why 'a plan with budgets and --turn T1, on a project with a ready host' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $budgetPlan, '--turn', 'T1') -Kind lease -State ready -Ttl 4080
    $idb = Get-VkId $d
    $d2 = Test-VkLine -Plan -Why 'a plan with budgets and no --turn, on a project with a ready host' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $budgetPlan) -Kind lease -State ready -Ttl 10800
    $idb2 = Get-VkId $d2
    if ($idb) {
      [void](Test-VkLine -Plan -Why 'a ready lease made with --turn, read back' -Arguments @('lease', 'get', $idb) -Kind lease -State ready -Ttl 4080)
      [void](Test-VkLine -Plan -Why 'a ready lease made with --turn, renewed with its own TTL' -Arguments @('lease', 'renew', $idb) -Kind lease -State ready -Ttl 4080)
      [void](Test-VkLine -Plan -Why 'a ready lease made with --turn, released' -Arguments @('lease', 'release', $idb) -Kind lease -State released)
    }
    if ($idb2) { [void](Test-VkLine -Plan -Why 'a ready lease made from a budget, released' -Arguments @('lease', 'release', $idb2) -Kind lease -State released) }
  } else { Skip-VkNoCa "lease-api.md#versioning: a ready lease made with --turn validates against v2's schema and v1's" }
  [void](Test-VkLine -Plan -Why 'every lease, in a list, the ones made with --turn among them' -Arguments @('lease', 'list') -Kind list)

  # ===== internal: no CA, and records that cannot be read =====
  if ($lab.CaReady) {
    $d = Test-VkLine -Why 'a ready lease, made before the CA is removed' -Arguments @('lease', 'create', '--project', 'v2c-ready') -Kind lease -State ready
    $idc = Get-VkId $d
    Remove-PvCa -Lab $lab
    [void](Test-VkLine -Why 'no CA: a create that would be ready' -Arguments @('lease', 'create', '--project', 'v2c-ready') -Kind error -Code internal)
    [void](Test-VkLine -Plan -Why 'no CA: a create from a plan that would be ready' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $midPlan) -Kind error -Code internal)
    [void](Test-VkLine -Plan -Why 'no CA: a plan no size holds needs none' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $bigPlan) -Kind lease -State unsatisfiable -Reasons @('plan_too_large'))
    if ($idc) {
      [void](Test-VkLine -Why 'no CA: a renew' -Arguments @('lease', 'renew', $idc) -Kind error -Code internal)
      [void](Test-VkLine -Why 'no CA: a release, which needs none' -Arguments @('lease', 'release', $idc) -Kind lease -State released)
    } else { foreach ($n in 'a renew', 'a release') { Write-ConformanceResult -Name "lease-api.md#versioning: $n with no CA validates against v0's schema, v1's and v2's" -Status FAIL -Reason 'There was no ready lease to read back' } }
  } else {
    foreach ($n in 'a create', 'a renew', 'a release') { Skip-VkNoCa "lease-api.md#versioning: $n with no CA validates against v0's schema, v1's and v2's" }
  }
  # The state file is not a document: every command that reads it fails with internal, and nothing is changed.
  $statePath = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.StateDir 'leases.json')
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $statePath), [Text.UTF8Encoding]::new($false).GetBytes("this is not a lease state file`n"))
  [void](Test-VkLine -Why 'the lease records cannot be read: a get' -Arguments @('lease', 'get', $notFound) -Kind error -Code internal)
  [void](Test-VkLine -Why 'the lease records cannot be read: a list' -Arguments @('lease', 'list') -Kind error -Code internal)
  [void](Test-VkLine -Why 'the lease records cannot be read: a create' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--platform', 'windows/amd64') -Kind error -Code internal)
  [void](Test-VkLine -Plan -Why 'the lease records cannot be read: a create from a plan' -Arguments @('lease', 'create', '--project', 'v2c-ready', '--plan', $midPlan, '--platform', 'windows/amd64') -Kind error -Code internal)
  Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $statePath) -Force

  # ===== Controls: what v1 and v2 added MUST fail the schemas that lack it, so the checks above are not vacuous =====
  # Each is run, and its document is validated against v2's, v1's and v0's, each by its own expectation.
  $controls = @(
    @{ Why = 'a lease made with --plan holds sized_for, which v0 never printed'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $okPlan); Kind = 'lease'; Exit = 0; V2 = $true; V1 = $true; V0 = $false }
    @{ Why = 'a lease whose plan no size holds has the reason plan_too_large and sized_for, which v0 never printed'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $bigPlan); Kind = 'lease'; Exit = 0; V2 = $true; V1 = $true; V0 = $false }
    @{ Why = '--plan with --size is a usage error for the field plan, which v0 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $okPlan, '--size', 'm'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $true; V0 = $false }
    @{ Why = 'a plan that is not a file is invalid_value for the field plan, which v0 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', 'missing.json'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $true; V0 = $false }
    @{ Why = '--board without --epic is a usage error for the field epic, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--board', $boardId); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = '--epic without --board is a usage error for the field board, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--epic', '7'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = '--turn without a plan is a usage error for the field turn, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--turn', 'T1'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = '--board with --plan is a usage error for the field board, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--board', $boardId, '--epic', '7', '--plan', $okPlan); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = '--epic with --size is a usage error for the field epic, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--epic', '7', '--size', 'm'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = '--turn with no value is a usage error for the field turn, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $okPlan, '--turn'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = 'a board that is not a UUID is invalid_value for the field board, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--board', 'not-a-board', '--epic', '7'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = 'an epic that is not a ticket number is invalid_value for the field epic, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--board', $boardId, '--epic', '#0052'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = 'a turn outside a turn name''s form is invalid_value for the field turn, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $okPlan, '--turn', 'T1 '); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
    @{ Why = 'a turn that the plan does not have is invalid_value for the field turn, which v1 never named'; A = @('lease', 'create', '--project', 'v2c-nocap', '--plan', $okPlan, '--turn', 'Nope'); Kind = 'error'; Exit = 2; V2 = $true; V1 = $false; V0 = $false }
  )
  foreach ($c in $controls) {
    $which = @(if ($c.V2) { "validates against v2's schema" } else { "fails v2's" }) + @(if ($c.V1) { "validates against v1's" } else { "fails v1's" }) + @(if ($c.V0) { "validates against v0's" } else { "fails v0's" })
    $name = "lease-api.md#schemas: Control: $($c.Why), so it $($which -join ', ')"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $run = Invoke-VkCli -Arguments $c.A
      $schemas = Get-VkSchemas $c.Kind
      $text = if ($c.Kind -ceq 'error') { [string]$run.Stderr } else { [string]$run.Stdout }
      if ($run.ExitCode -ne $c.Exit) { $p.Add("Exit $($run.ExitCode), not $($c.Exit)") }
      if ([string]::IsNullOrWhiteSpace($text)) { $p.Add('There is no document') }
      else {
        foreach ($v in @(@('V2', $c.V2), @('V1', $c.V1), @('V0', $c.V0))) {
          $ok = Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema $schemas[$v[0]]
          if ($ok -and -not $v[1]) { $p.Add("The document validates against $($schemas[$v[0]]), which has no place for what was added after it") }
          if (-not $ok -and $v[1]) { $p.Add("The document does not validate against $($schemas[$v[0]])") }
        }
      }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-VkCheck -Name $name -Problems $p
  }

  # --- Every state, reason and error code was seen, and enough documents were made ---
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($s in 'ready', 'pending', 'expired', 'released', 'unsatisfiable') { if (-not $script:VkStates.ContainsKey($s)) { $p.Add("No lease in the state $s was validated") } }
  foreach ($r in 'unsupported_platform', 'unsupported_profile', 'unsupported_size', 'no_capacity', 'plan_too_large') { if (-not $script:VkReasons.ContainsKey($r)) { $p.Add("No lease with the reason $r was validated") } }
  foreach ($c in 'usage', 'invalid_value', 'out_of_range', 'not_found', 'invalid_state', 'internal') { if (-not $script:VkCodes.ContainsKey($c)) { $p.Add("No error with the code $c was validated") } }
  if ($script:VkDocs -lt 130) { $p.Add("Only $($script:VkDocs) documents were validated, not at least 130") }
  if ($p.Count -gt 0 -and -not $lab.CaReady) { Write-ConformanceResult -Name 'lease-api.md#versioning: Every state, reason and error code was printed and validated' -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)" }
  else { Complete-VkCheck -Name 'lease-api.md#versioning: Every state, every reason and every error code was printed and validated against v2''s schemas and v1''s, in at least 130 documents' -Problems $p }
} catch {
  Write-ConformanceResult -Name 'test-v2-compatible.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(Format-VkText $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  [Environment]::CurrentDirectory = $savedCwd
  if ($lab) {
    try { Clear-PvCleanup -Lab $lab } catch { }
    Remove-ConformanceHome -ConformanceHome $lab
  }
}
Exit-ConformanceRun

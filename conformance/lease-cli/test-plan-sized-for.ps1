#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what
# docs/lease-api.md documents, with a stand-in for the provisioning run (a program outside agentspace). See
# docs/conformance.md. A v0.10 build MUST pass every result line.
#
# Cases of E0.10-T04-S01 (card #0221), `sized_for` (lease-api.md, "Fields"; D75, D76). `sized_for` is
# {"plan","turn","agents"}: the plan's name as written (left out, never null, when the plan has none), the
# busiest turn's name as written, and that turn's agents at once. `create` sets it, nothing changes it
# afterwards, and every command that prints a lease prints it as `create` did, in every state; a lease made
# without --plan has none. Every document, a list's entries included, is validated against v1's schemas
# (lease.output.schema.json, lease-list.output.schema.json) by the harness's Test-ConformanceSchema, which
# cannot tell a missing `sized_for` from a lease that has none, so each result reads the member itself.
#
#   Ready, expired, released   A plan lease made ready on a record ready at its size's type, then read by get and
#             list, renewed, released, read again, and released again; a second one whose expires_at is moved into
#             the past, as the suite does for expiry (an expired lease is reached by time, which a check cannot
#             wait out), read by get and list as expired and released from expired; and a lease with no plan
#             alongside, whose documents have none. No output prints the plan, a story, a subagent, a model or an
#             effort.
#   Pending   A plan whose size the host is not at, with no live lease, answers pending (driver.md, "On a `ready`
#             host", row 6): create, get, list and release print it, and so does a lease with no plan made beside it.
#   Unsatisfiable   no_capacity (a live ready lease on a smaller host, row 5) and unsupported_platform, each read by
#             create, get, list and release, which prints the lease unchanged; and a lease with no plan.
#   Names     A plan with no name and one whose name is null have a sized_for with no `plan`; names with spaces,
#             quotes, a backslash, mixed case and the longest form of 64 characters are printed as the plan writes
#             them.
# Not here, and another file's: plan_too_large and its sized_for, plan sizing itself (test-plan-sizes.ps1), a plan
# that is not well formed, and v0 compatibility.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts a
# real run: every create is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it
# is given and makes no host. No program of agentspace's but the lease CLI is started, and only through the
# harness. Every write, the plan files, the host registry and the one record's expires_at included, calls
# Assert-ConformancePath at the call (Write-PvText, Set-ConformanceRecordMember). No failure reason quotes a path
# or a value the program printed.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-plan-sized-for.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

# A project of the cases' own, with a record, that is not swimcoach.
$script:PlProject = 'sizedcase'
# The four sizes' types (driver.md, "Sizes and instance types"), the cases' own data for a host record.
$script:PlType = @{ s = 'm9g.large'; m = 'm9g.xlarge'; l = 'm9g.2xlarge'; xl = 'm9g.4xlarge' }
# The box's record, first in every registry written (state.md, "The host registry").
$script:PlBox = '{"project":"swimcoach","stack":"agentbox","state":"ready","instance_type":"m9g.xlarge"}'
# The names, models and efforts of the plans below that no output may print: a plan, a story, a subagent, a
# model and an effort never reach an output (lease-api.md, "Plan" and "Fields"). Each is unlike any word of a
# lease document.
$script:PlLeakTokens = @('Zstory-', 'zsub', 'Qmodel', 'qeffort9')

# --- A lab that reports each result by its whole name ------------------------------------------------------

# Like Use-PvLab, but the body's dictionary keys are whole result names, so one home can show results under
# several anchors of lease-api.md. $Name names the check when it throws or is skipped.
function Use-PlLab {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][scriptblock]$Body, [switch]$NoCa)
  $lab = $null
  $script:PvCleanup.Clear()
  $savedCwd = [Environment]::CurrentDirectory
  try {
    $lab = New-ConformanceHome
    if (-not $NoCa -and -not $lab.CaReady) {
      Write-ConformanceResult -Name $Name -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
      return
    }
    Reset-PvState -Lab $lab
    $out = @(& $Body $lab)
    $many = $out | Where-Object { $_ -is [System.Collections.IDictionary] } | Select-Object -Last 1
    if ($null -eq $many) { Write-ConformanceResult -Name $Name -Status FAIL -Reason 'The check body returned no results' }
    else { foreach ($key in $many.Keys) { Complete-ConformanceCheck -Name $key -Problems $many[$key] } }
  } catch {
    $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $Name -Status FAIL -Reason "It threw: $why at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    [Environment]::CurrentDirectory = $savedCwd
    if ($lab) { Clear-PvCleanup -Lab $lab }
    if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
  }
}

# --- What the cases write ------------------------------------------------------------------------------------

# The host registry's text: the box's record, then the project's, ready at the type given.
function Set-PlHost {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$InstanceType)
  $record = '{"project":"' + $script:PlProject + '","stack":"agentspace-host-' + $script:PlProject + '","state":"ready","instance_type":"' + $InstanceType + '"}'
  $text = '{"version":0,"hosts":[' + $script:PlBox + ',' + $record + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The case wrote a hosts.json that does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
}

# A JSON string for a name: the two characters that need an escape, as printable ASCII names can hold them.
function Format-PlString { param([string]$Text) '"' + $Text.Replace('\', '\\').Replace('"', '\"') + '"' }

function New-PlSubagent { param([Parameter(Mandatory)][string]$Name, [string]$Model = 'Qmodel 4.2', [string]$Effort = 'qeffort9') '{"name":' + (Format-PlString $Name) + ',"model":' + (Format-PlString $Model) + ',"effort":' + (Format-PlString $Effort) + '}' }
function New-PlStory { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Subagents) '{"name":' + (Format-PlString $Name) + ',"subagents":[' + ($Subagents -join ',') + ']}' }
function New-PlTurn { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Stories) '{"name":' + (Format-PlString $Name) + ',"stories":[' + ($Stories -join ',') + ']}' }
function New-PlPlan {
  param([Parameter(Mandatory)][string[]]$Turns, [string]$Name, [switch]$NullName)
  $s = '{'
  if ($NullName) { $s += '"name":null,' } elseif ($PSBoundParameters.ContainsKey('Name')) { $s += '"name":' + (Format-PlString $Name) + ',' }
  return $s + '"turns":[' + ($Turns -join ',') + ']}'
}

# A turn of one story per peak given, each story of that many subagents none starting after another: the turn's
# agents at once are the peaks' sum plus 1 for its dev lead.
function New-PlPeaksTurn {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int[]]$Peaks)
  $i = 0
  $stories = foreach ($peak in $Peaks) {
    $i++
    New-PlStory -Name "Zstory-$i" -Subagents @(foreach ($j in 1..$peak) { New-PlSubagent -Name "zsub$j" })
  }
  New-PlTurn -Name $Name -Stories $stories
}

# Writes a plan file in the home, checks that it is a plan by its schema (the case's own mistake otherwise), and
# returns the path to give --plan.
function Write-PlPlan {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$File, [Parameter(Mandatory)][string]$Text)
  if (-not (Test-ConformanceSchema -Json $Text -Schema 'plan.input.schema.json')) { throw "The case wrote a plan, $File, that does not validate against its schema." }
  $path = Join-Path $Lab.Root "plans\$File.json"
  Write-PvText -Lab $Lab -Path $path -Text $Text
  return (Assert-ConformancePath -ConformanceHome $Lab -Path $path)
}

# --- What a case compares ------------------------------------------------------------------------------------

# The problems with a document's sized_for. With no $Expect the lease was made without --plan, and the member is
# absent. Otherwise it is present, an object of exactly turn and agents (and plan when the plan has a name), each as
# the plan writes it. $Expect is @{ Plan; Turn; Agents }, Plan $null for a plan with no name.
function Get-PlSizedForProblems {
  param([Parameter(Mandatory)]$Doc, $Expect)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($null -eq $Expect) {
    if ($Doc.ContainsKey('sized_for')) { $p.Add('A lease made without --plan has a sized_for') }
    return , $p
  }
  if (-not $Doc.ContainsKey('sized_for')) { $p.Add('The lease has no sized_for'); return , $p }
  $sf = $Doc['sized_for']
  if ($sf -isnot [hashtable]) { $p.Add('sized_for is not an object'); return , $p }
  $want = @('agents', 'turn')
  if ($null -ne $Expect.Plan) { $want += 'plan' }
  $got = @($sf.Keys | ForEach-Object { [string]$_ } | Sort-Object { $_ } -CaseSensitive)
  $want = @($want | Sort-Object { $_ } -CaseSensitive)
  if (($got -join ',') -cne ($want -join ',')) { $p.Add("sized_for has the members [$($got -join ', ')], not [$($want -join ', ')]") }
  if ($sf.ContainsKey('turn') -and ($sf['turn'] -isnot [string] -or $sf['turn'] -cne $Expect.Turn)) { $p.Add('sized_for.turn is not the busiest turn''s name as the plan writes it') }
  if ($null -ne $Expect.Plan -and $sf.ContainsKey('plan') -and ($sf['plan'] -isnot [string] -or $sf['plan'] -cne $Expect.Plan)) { $p.Add('sized_for.plan is not the plan''s name as the plan writes it') }
  if ($sf.ContainsKey('agents')) {
    $a = $sf['agents']
    if (($a -isnot [int] -and $a -isnot [long]) -or $a -ne $Expect.Agents) { $p.Add("sized_for.agents is not the integer $($Expect.Agents)") }
  }
  return , $p
}

# The problems with one lease, as a document or as a list's entry: its state, project and size, its sized_for,
# and, when $SameAs is given, a sized_for that is as create printed it (canonical text, so by content).
function Get-PlLeaseProblems {
  param([Parameter(Mandatory)]$Doc, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size, $Expect, [string]$SameAs)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Doc['state'] -cne $State) { $p.Add("The lease's state is '$($Doc['state'])', not $State") }
  if ($Doc['project'] -cne $script:PlProject) { $p.Add('The lease is for another project') }
  if ($Doc['size'] -cne $Size) { $p.Add("The lease's size is '$($Doc['size'])', not '$Size'") }
  foreach ($x in (Get-PlSizedForProblems -Doc $Doc -Expect $Expect)) { $p.Add($x) }
  if ($SameAs -and $Doc.ContainsKey('sized_for') -and (ConvertTo-CcCanonical $Doc['sized_for']) -cne $SameAs) { $p.Add('sized_for is not as create printed it') }
  return , $p
}

# The problems with a run that printed one lease document: the success test, its schema, and the lease's.
function Get-PlDocProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size, $Expect, [string]$SameAs)
  $p = Get-ConformanceSuccessProblems -Run $Run
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  if ($doc['id'] -cne $Id) { $p.Add('The document is another lease''s') }
  foreach ($x in (Get-PlLeaseProblems -Doc $doc -State $State -Size $Size -Expect $Expect -SameAs $SameAs)) { $p.Add($x) }
  return , $p
}

# The problems with a list run: the success test against the list schema, and the one entry of the lease given.
function Get-PlListProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Id, [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size, $Expect, [string]$SameAs)
  $p = Get-ConformanceSuccessProblems -Run $Run -Schema 'lease-list.output.schema.json'
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  $entries = @($doc['leases'] | Where-Object { $_ -is [hashtable] -and $_['id'] -ceq $Id })
  if ($entries.Count -ne 1) { $p.Add("The list holds $($entries.Count) entries of the lease, not 1"); return , $p }
  foreach ($x in (Get-PlLeaseProblems -Doc $entries[0] -State $State -Size $Size -Expect $Expect -SameAs $SameAs)) { $p.Add($x) }
  return , $p
}

# The problems with a ready answer: the document's, and the lease's stored record in the state it is.
function Get-PlReadyProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, $Expect)
  $p = Get-ConformanceSuccessProblems -Run $Run
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  foreach ($x in (Get-PlLeaseProblems -Doc $doc -State 'ready' -Size $Size -Expect $Expect)) { $p.Add($x) }
  if ($doc['ssh'] -is [hashtable] -and [string]$doc['ssh']['host_alias'] -cmatch "\Aagentspace-$([regex]::Escape($script:PlProject))-[0-9a-f]{8}\z") {
    $rec = Get-CcRecord $Lab ([string]$doc['id'])
    if ($null -eq $rec -or $rec['state'] -cne 'ready' -or $rec['size'] -cne $Size) { $p.Add('The lease is not in the lease state file as a ready lease of the size') }
  } else { $p.Add('The ready lease has no host alias of the project''s form') }
  return , $p
}

# The problems with a run that answered pending: the document's, with the lease's.
function Get-PlPendingProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, $Expect)
  $p = Get-PvPendingDocProblems -Run $Run -Project $script:PlProject
  if ($Run.Json -is [hashtable]) {
    if ($Run.Json['size'] -cne $Size) { $p.Add("The lease's size is '$($Run.Json['size'])', not '$Size'") }
    foreach ($x in (Get-PlSizedForProblems -Doc $Run.Json -Expect $Expect)) { $p.Add($x) }
  }
  return , $p
}

# The problems with a run that answered unsatisfiable with the reason codes given, in that order, for a lease of
# the size, with the sized_for expected, or none.
function Get-PlUnsatisfiableProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string[]]$Codes, [Parameter(Mandatory)][string]$Size, $Expect)
  $p = Get-PvUnsatisfiableDocProblems -Run $Run -Project $script:PlProject -Codes $Codes
  if ($Run.Json -is [hashtable]) {
    if ($Run.Json['size'] -cne $Size) { $p.Add("The lease's size is '$($Run.Json['size'])', not '$Size'") }
    foreach ($x in (Get-PlSizedForProblems -Doc $Run.Json -Expect $Expect)) { $p.Add($x) }
  }
  return , $p
}

# The problems with outputs that print the plan: a story, a subagent, a model or an effort of a plan above.
function Get-PlLeakProblems {
  param([Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][System.Collections.Generic.List[string]]$Outputs)
  $p = [System.Collections.Generic.List[string]]::new()
  $text = $Outputs -join "`n"
  if ($text.Trim().Length -eq 0) { $p.Add('There was no output to look at'); return , $p }
  foreach ($t in $script:PlLeakTokens) { if ($text.Contains($t)) { $p.Add("An output prints a word of the plan that no output prints, '$t'") } }
  return , $p
}

# Runs a lease command in the case's home with its stand-in, keeping the stdout for the leak check.
function Invoke-PlLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments, [Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][System.Collections.Generic.List[string]]$Outputs)
  $r = Invoke-PvLease -Lab $Lab -Arguments $Arguments -StandIn $StandIn
  $Outputs.Add([string]$r.Stdout)
  return $r
}

# The ID of the lease a run printed, when it printed a lease in the state given and an ID that is a lowercase
# UUIDv4, and $null otherwise. The ID is only ever given back to the program as an argument, never built into a path.
function Get-PlLeaseId {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$State)
  $doc = $Run.Json
  if ($doc -is [hashtable] -and $doc['state'] -ceq $State -and [string]$doc['id'] -cmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') { return [string]$doc['id'] }
  return $null
}

# The canonical text of the sized_for a create printed, to compare what the later commands print with it, or an
# empty text when it printed none (the case's own problems then say so).
function Get-PlSameAs {
  param([Parameter(Mandatory)]$Run)
  if ($Run.Json -is [hashtable] -and $Run.Json.ContainsKey('sized_for')) { return (ConvertTo-CcCanonical $Run.Json['sized_for']) }
  return ''
}

# One later command on a lease, and the problems with what it printed: its document's, or a list's entry of it.
# With no ID, the create before it made no lease to read, and that is the one problem. $Whole, when given, is the
# canonical text of the whole document create printed, which the command MUST print again (release of an
# unsatisfiable lease).
function Get-PlStepProblems {
  param(
    [Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][System.Collections.Generic.List[string]]$Outputs,
    [AllowNull()][string]$Id, [Parameter(Mandatory)][ValidateSet('get', 'renew', 'release', 'list')][string]$Command, [string]$ListState,
    [Parameter(Mandatory)][string]$State, [Parameter(Mandatory)][string]$Size, $Expect, [string]$SameAs, [string]$Whole
  )
  if (-not $Id) {
    $p = [System.Collections.Generic.List[string]]::new()
    $p.Add('The create before it printed no lease in the state wanted, so there was no lease to read')
    return , $p
  }
  if ($Command -ceq 'list') {
    $a = @('list', '--project', $script:PlProject)
    if ($ListState) { $a += @('--state', $ListState) }
    $r = Invoke-PlLease -Lab $Lab -Arguments $a -StandIn $StandIn -Outputs $Outputs
    $p = Get-PlListProblems -Run $r -Id $Id -State $State -Size $Size -Expect $Expect -SameAs $SameAs
    return , $p
  }
  $r = Invoke-PlLease -Lab $Lab -Arguments @($Command, $Id) -StandIn $StandIn -Outputs $Outputs
  $p = Get-PlDocProblems -Run $r -Id $Id -State $State -Size $Size -Expect $Expect -SameAs $SameAs
  if ($Whole -and $r.Json -is [hashtable] -and (ConvertTo-CcCanonical $r.Json) -cne $Whole) { $p.Add("$Command printed a lease other than the one create printed") }
  return , $p
}

# The problems with a stand-in that was never started. A program that prints nothing starts nothing, so the result
# also needs every create of the home to have printed a lease document, which says the creates ran.
function Get-PlNoStartProblems {
  param([Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][object[]]$Runs)
  $p = Get-PvNoStartProblems -StandIn $StandIn
  $docs = @($Runs | Where-Object { $_.Json -is [hashtable] }).Count
  if ($docs -ne $Runs.Count) { $p.Add("Only $docs of the $($Runs.Count) creates printed a lease document, so what they started is not known") }
  return , $p
}

# The plans of the cases. P1: three turns, the busiest 'T04 build' with two stories (peaks 2 and 3), 6 agents, size m.
$script:PlP1 = New-PlPlan -Name 'Epic v0.10 sized' -Turns @((New-PlPeaksTurn 'T01 small' @(2)), (New-PlPeaksTurn 'T04 build' @(2, 3)), (New-PlPeaksTurn 'T09 tail' @(1)))
$script:PlE1 = @{ Plan = 'Epic v0.10 sized'; Turn = 'T04 build'; Agents = 6 }
# P2 and P3: one turn of 12 agents, size l. P4: one turn of 4 agents, size s.
$script:PlP2 = New-PlPlan -Name 'Epic pending' -Turns @((New-PlPeaksTurn 'Wide turn' @(11)))
$script:PlE2 = @{ Plan = 'Epic pending'; Turn = 'Wide turn'; Agents = 12 }
$script:PlP3 = New-PlPlan -Name 'Epic capacity' -Turns @((New-PlPeaksTurn 'Crowded turn' @(11)))
$script:PlE3 = @{ Plan = 'Epic capacity'; Turn = 'Crowded turn'; Agents = 12 }
$script:PlP4 = New-PlPlan -Name 'Epic platform' -Turns @((New-PlPeaksTurn 'Platform turn' @(3)))
$script:PlE4 = @{ Plan = 'Epic platform'; Turn = 'Platform turn'; Agents = 4 }

# ---- Ready, expired and released ----------------------------------------------------------------------------
Use-PlLab -Name 'lease-api.md#fields: A lease made with --plan prints sized_for as ready, expired and released' -Body {
  param($lab)
  Set-PlHost -Lab $lab -InstanceType $script:PlType['m']
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $outputs = [System.Collections.Generic.List[string]]::new()
  $path = Write-PlPlan -Lab $lab -File 'p1' -Text $script:PlP1
  $project = $script:PlProject
  $step = @{ Lab = $lab; StandIn = $standIn; Outputs = $outputs }

  # A: made ready, read, renewed, released, read again, released again.
  $a = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project, '--plan', $path) -StandIn $standIn -Outputs $outputs
  $results['lease-api.md#agentspace-lease-create: create --plan answers ready, and sized_for is the plan''s name, its busiest turn and its 6 agents at once, with size m, and no other member'] = Get-PlReadyProblems -Lab $lab -Run $a -Size 'm' -Expect $script:PlE1
  $id = Get-PlLeaseId -Run $a -State 'ready'
  $same = Get-PlSameAs -Run $a
  $results['lease-api.md#agentspace-lease-get: get prints a ready lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'get' -State 'ready' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-list: list prints a ready lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'list' -ListState 'ready' -State 'ready' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-renew: renew prints the renewed lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'renew' -State 'ready' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-release: release prints a ready lease, now released, with the sized_for create printed'] = Get-PlStepProblems @step -Id $id -Command 'release' -State 'released' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-get: get prints a released lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'get' -State 'released' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-list: list prints a released lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'list' -ListState 'released' -State 'released' -Size 'm' -Expect $script:PlE1 -SameAs $same
  $results['lease-api.md#agentspace-lease-release: release of a released lease prints its sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'release' -State 'released' -Size 'm' -Expect $script:PlE1 -SameAs $same

  # B: made ready, then expired by moving its expires_at into the past, read, and released from expired.
  $b = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project, '--plan', $path) -StandIn $standIn -Outputs $outputs
  $idB = Get-PlLeaseId -Run $b -State 'ready'
  $sameB = Get-PlSameAs -Run $b
  $seeded = $false
  if ($idB) { $seeded = Set-ConformanceRecordMember -ConformanceHome $lab -Id $idB -Member 'expires_at' -Value '2020-01-01T00:00:00Z' }
  $p = Get-PlStepProblems @step -Id $idB -Command 'get' -State 'expired' -Size 'm' -Expect $script:PlE1 -SameAs $sameB
  if ($idB -and -not $seeded) { $p.Add('The case could not move the lease''s expires_at, so the lease is not expired') }
  $results['lease-api.md#agentspace-lease-get: get prints an expired lease''s sized_for as create did'] = $p
  $results['lease-api.md#agentspace-lease-list: list prints an expired lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $idB -Command 'list' -ListState 'expired' -State 'expired' -Size 'm' -Expect $script:PlE1 -SameAs $sameB
  $results['lease-api.md#agentspace-lease-release: release prints an expired lease, now released, with the sized_for create printed'] = Get-PlStepProblems @step -Id $idB -Command 'release' -State 'released' -Size 'm' -Expect $script:PlE1 -SameAs $sameB

  # N: a lease made without --plan, in the same home, in each command.
  $n = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project) -StandIn $standIn -Outputs $outputs
  $idN = Get-PlLeaseId -Run $n -State 'ready'
  $results['lease-api.md#fields: A ready lease made without --plan has no sized_for in the document create prints'] = Get-PlReadyProblems -Lab $lab -Run $n -Size 'm' -Expect $null
  $results['lease-api.md#fields: get prints a lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'get' -State 'ready' -Size 'm' -Expect $null
  $results['lease-api.md#fields: list prints a lease made without --plan, beside leases made with one, with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'list' -ListState 'ready' -State 'ready' -Size 'm' -Expect $null
  $results['lease-api.md#fields: renew prints a lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'renew' -State 'ready' -Size 'm' -Expect $null
  $results['lease-api.md#fields: release prints a lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'release' -State 'released' -Size 'm' -Expect $null

  $results['lease-api.md#fields: No output prints the plan: no story, subagent, model or effort of it is in any document of these commands'] = Get-PlLeakProblems -Outputs $outputs
  $results['lease-api.md#agentspace-lease-create: Creating ready leases of an exact host starts nothing'] = Get-PlNoStartProblems -StandIn $standIn -Runs @($a, $b, $n)
  return $results
}

# ---- Pending ------------------------------------------------------------------------------------------------
# A host at m and no live lease: a plan of 12 agents, size l, is pending (driver.md, "On a `ready` host", row 6),
# and so is a lease with no plan made while it is (row 1). A get MAY start the stand-in, which makes no host.
Use-PlLab -Name 'lease-api.md#fields: A lease made with --plan prints sized_for as pending' -Body {
  param($lab)
  Set-PlHost -Lab $lab -InstanceType $script:PlType['m']
  $standIn = New-PvStandIn -Lab $lab -Name 'run'
  $results = [ordered]@{}
  $outputs = [System.Collections.Generic.List[string]]::new()
  $path = Write-PlPlan -Lab $lab -File 'p2' -Text $script:PlP2
  $project = $script:PlProject
  $step = @{ Lab = $lab; StandIn = $standIn; Outputs = $outputs }

  $a = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project, '--plan', $path) -StandIn $standIn -Outputs $outputs
  $results['lease-api.md#agentspace-lease-create: create --plan answers pending for a host at a smaller size, with size l, and sized_for is the plan''s name, its turn and its 12 agents at once'] = Get-PlPendingProblems -Run $a -Size 'l' -Expect $script:PlE2
  $n = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project, '--size', 'l') -StandIn $standIn -Outputs $outputs
  $results['lease-api.md#fields: A pending lease made without --plan has no sized_for in the document create prints'] = Get-PlPendingProblems -Run $n -Size 'l' -Expect $null
  $id = Get-PlLeaseId -Run $a -State 'pending'
  $same = Get-PlSameAs -Run $a
  $results['lease-api.md#agentspace-lease-get: get prints a pending lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'get' -State 'pending' -Size 'l' -Expect $script:PlE2 -SameAs $same
  $results['lease-api.md#agentspace-lease-list: list prints a pending lease''s sized_for as create did'] = Get-PlStepProblems @step -Id $id -Command 'list' -ListState 'pending' -State 'pending' -Size 'l' -Expect $script:PlE2 -SameAs $same
  $results['lease-api.md#agentspace-lease-release: release gives up a pending lease, now released, and prints the sized_for create printed'] = Get-PlStepProblems @step -Id $id -Command 'release' -State 'released' -Size 'l' -Expect $script:PlE2 -SameAs $same
  $results['lease-api.md#agentspace-lease-get: get prints a lease released from pending with the sized_for create printed'] = Get-PlStepProblems @step -Id $id -Command 'get' -State 'released' -Size 'l' -Expect $script:PlE2 -SameAs $same
  $results['lease-api.md#agentspace-lease-list: list prints a lease released from pending with the sized_for create printed'] = Get-PlStepProblems @step -Id $id -Command 'list' -ListState 'released' -State 'released' -Size 'l' -Expect $script:PlE2 -SameAs $same
  $idN = Get-PlLeaseId -Run $n -State 'pending'
  $results['lease-api.md#fields: get prints a pending lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'get' -State 'pending' -Size 'l' -Expect $null
  $results['lease-api.md#fields: list prints a pending lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'list' -ListState 'pending' -State 'pending' -Size 'l' -Expect $null
  $results['lease-api.md#fields: release prints a pending lease made without --plan, now released, with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'release' -State 'released' -Size 'l' -Expect $null
  $results['lease-api.md#fields: No output prints the plan: no story, subagent, model or effort of it is in any document of the pending leases'] = Get-PlLeakProblems -Outputs $outputs
  return $results
}

# ---- Unsatisfiable ------------------------------------------------------------------------------------------
# A live ready lease with no plan holds a host at m, so a plan of 12 agents, size l, is refused with no_capacity
# (row 5). A platform no backend has makes any lease unsatisfiable with unsupported_platform. A release of an
# unsatisfiable lease prints it unchanged.
Use-PlLab -Name 'lease-api.md#fields: A lease made with --plan prints sized_for as unsatisfiable' -Body {
  param($lab)
  Set-PlHost -Lab $lab -InstanceType $script:PlType['m']
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $outputs = [System.Collections.Generic.List[string]]::new()
  $capPath = Write-PlPlan -Lab $lab -File 'p3' -Text $script:PlP3
  $platPath = Write-PlPlan -Lab $lab -File 'p4' -Text $script:PlP4
  $project = $script:PlProject
  $step = @{ Lab = $lab; StandIn = $standIn; Outputs = $outputs }
  $runs = [System.Collections.Generic.List[object]]::new()

  $held = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project) -StandIn $standIn -Outputs $outputs
  $runs.Add($held)
  $results['lease-api.md#fields: The live ready lease of the project that holds the host is made without --plan, so it has no sized_for'] = Get-PlReadyProblems -Lab $lab -Run $held -Size 'm' -Expect $null

  # Each case: the arguments, the reason codes, the size, and the sized_for expected.
  $cases = @(
    @{ Key = 'no_capacity'; Extra = @('--plan', $capPath); Codes = @('no_capacity'); Size = 'l'; Expect = $script:PlE3; Words = 'the host is held by a live ready lease at a smaller size' }
    @{ Key = 'unsupported_platform'; Extra = @('--plan', $platPath, '--platform', 'windows/amd64'); Codes = @('unsupported_platform'); Size = 's'; Expect = $script:PlE4; Words = 'the platform is one no backend has' }
  )
  foreach ($c in $cases) {
    $a = Invoke-PlLease -Lab $lab -Arguments (@('create', '--project', $project) + $c.Extra) -StandIn $standIn -Outputs $outputs
    $runs.Add($a)
    $results["lease-api.md#agentspace-lease-create: create --plan answers unsatisfiable with $($c.Key) alone when $($c.Words), with size $($c.Size) and its sized_for"] = Get-PlUnsatisfiableProblems -Run $a -Codes $c.Codes -Size $c.Size -Expect $c.Expect
    $id = Get-PlLeaseId -Run $a -State 'unsatisfiable'
    $same = Get-PlSameAs -Run $a
    $whole = $(if ($id) { ConvertTo-CcCanonical $a.Json } else { '' })
    $results["lease-api.md#agentspace-lease-get: get prints a lease unsatisfiable with $($c.Key) with the sized_for create printed"] = Get-PlStepProblems @step -Id $id -Command 'get' -State 'unsatisfiable' -Size $c.Size -Expect $c.Expect -SameAs $same
    $results["lease-api.md#agentspace-lease-list: list prints a lease unsatisfiable with $($c.Key) with the sized_for create printed"] = Get-PlStepProblems @step -Id $id -Command 'list' -ListState 'unsatisfiable' -State 'unsatisfiable' -Size $c.Size -Expect $c.Expect -SameAs $same
    $results["lease-api.md#agentspace-lease-release: release prints a lease unsatisfiable with $($c.Key) unchanged, with the sized_for create printed"] = Get-PlStepProblems @step -Id $id -Command 'release' -State 'unsatisfiable' -Size $c.Size -Expect $c.Expect -SameAs $same -Whole $whole
  }

  # A lease made without --plan that is unsatisfiable has none, in each command.
  $n = Invoke-PlLease -Lab $lab -Arguments @('create', '--project', $project, '--platform', 'windows/amd64') -StandIn $standIn -Outputs $outputs
  $runs.Add($n)
  $idN = Get-PlLeaseId -Run $n -State 'unsatisfiable'
  $results['lease-api.md#fields: An unsatisfiable lease made without --plan has no sized_for in the document create prints'] = Get-PlUnsatisfiableProblems -Run $n -Codes @('unsupported_platform') -Size 'm' -Expect $null
  $results['lease-api.md#fields: get prints an unsatisfiable lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'get' -State 'unsatisfiable' -Size 'm' -Expect $null
  $results['lease-api.md#fields: list prints an unsatisfiable lease made without --plan, beside leases made with one, with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'list' -ListState 'unsatisfiable' -State 'unsatisfiable' -Size 'm' -Expect $null
  $results['lease-api.md#fields: release prints an unsatisfiable lease made without --plan with no sized_for'] = Get-PlStepProblems @step -Id $idN -Command 'release' -State 'unsatisfiable' -Size 'm' -Expect $null
  $results['lease-api.md#fields: No output prints the plan: no story, subagent, model or effort of it is in any document of the unsatisfiable leases'] = Get-PlLeakProblems -Outputs $outputs
  $results['lease-api.md#agentspace-lease-create: Unsatisfiable answers on a host held at a smaller size start nothing'] = Get-PlNoStartProblems -StandIn $standIn -Runs @($runs)
  return $results
}

# ---- Names ---------------------------------------------------------------------------------------------------
# sized_for.plan is left out, never null, when the plan has no name or a null one; both names are printed as the
# plan writes them, spaces, quotes, a backslash and case included, up to the longest form of 64 characters. The
# platform no backend has makes each lease unsatisfiable, so that no host or CA is needed.
Use-PlLab -Name 'lease-api.md#fields: sized_for names' -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $project = $script:PlProject
  $uuid = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'
  $runs = [System.Collections.Generic.List[object]]::new()
  $long = 'A' + (' b' * 31) + 'c'
  if ($long.Length -ne 64) { throw 'The case''s 64-character name is not 64 characters.' }
  $cases = @(
    @{ Key = 'A plan with no name has a sized_for with only turn and agents, and no plan member, not even a null one'; File = 'n1'
       Text = (New-PlPlan -Turns @((New-PlPeaksTurn 'Unnamed turn' @(2)))); Expect = @{ Plan = $null; Turn = 'Unnamed turn'; Agents = 3 }; NoPlanMember = $true }
    @{ Key = 'A plan whose name is null has a sized_for with only turn and agents, and no plan member, not even a null one'; File = 'n2'
       Text = (New-PlPlan -NullName -Turns @((New-PlPeaksTurn 'Null name turn' @(3)))); Expect = @{ Plan = $null; Turn = 'Null name turn'; Agents = 4 }; NoPlanMember = $true }
    @{ Key = 'The plan''s and the turn''s names are printed as the plan writes them, with spaces, quotes, a backslash, punctuation and mixed case'; File = 'n3'
       Text = (New-PlPlan -Name 'MiXed "Quoted" \ Epic: v0.10 & (co.)' -Turns @((New-PlPeaksTurn 'T04 "x" \ y; Z/z' @(2)))); Expect = @{ Plan = 'MiXed "Quoted" \ Epic: v0.10 & (co.)'; Turn = 'T04 "x" \ y; Z/z'; Agents = 3 }; NoPlanMember = $false }
    @{ Key = 'The plan''s and the turn''s names of the longest form, 64 characters, are printed whole'; File = 'n4'
       Text = (New-PlPlan -Name $long -Turns @((New-PlPeaksTurn $long @(2)))); Expect = @{ Plan = $long; Turn = $long; Agents = 3 }; NoPlanMember = $false }
    @{ Key = 'A name of one character is printed whole, and the turn is the busiest of two, not the first'; File = 'n5'
       Text = (New-PlPlan -Name 'p' -Turns @((New-PlPeaksTurn 'a' @(1)), (New-PlPeaksTurn 'b' @(2)))); Expect = @{ Plan = 'p'; Turn = 'b'; Agents = 3 }; NoPlanMember = $false }
  )
  foreach ($c in $cases) {
    $path = Write-PlPlan -Lab $lab -File $c.File -Text $c.Text
    $a = Invoke-PvLease -Lab $lab -Arguments @('create', '--project', $project, '--plan', $path, '--platform', 'windows/amd64') -StandIn $standIn
    $runs.Add($a)
    $p = Get-PlUnsatisfiableProblems -Run $a -Codes @('unsupported_platform') -Size 's' -Expect $c.Expect
    if ($c.NoPlanMember -and $a.Stdout -match '"plan"\s*:') { $p.Add('create printed a plan member in sized_for') }
    if ($a.Json -is [hashtable] -and [string]$a.Json['id'] -cmatch $uuid -and $a.Json['state'] -ceq 'unsatisfiable') {
      $id = [string]$a.Json['id']
      $same = ConvertTo-CcCanonical $a.Json['sized_for']
      $g = Invoke-PvLease -Lab $lab -Arguments @('get', $id) -StandIn $standIn
      foreach ($x in (Get-PlDocProblems -Run $g -Id $id -State 'unsatisfiable' -Size 's' -Expect $c.Expect -SameAs $same)) { $p.Add("get: $x") }
      if ($c.NoPlanMember -and $g.Stdout -match '"plan"\s*:') { $p.Add('get printed a plan member in sized_for') }
      $l = Invoke-PvLease -Lab $lab -Arguments @('list', '--project', $project) -StandIn $standIn
      foreach ($x in (Get-PlListProblems -Run $l -Id $id -State 'unsatisfiable' -Size 's' -Expect $c.Expect -SameAs $same)) { $p.Add("list: $x") }
    }
    $results["lease-api.md#fields: $($c.Key), in create, get and list"] = $p
  }
  $results['lease-api.md#agentspace-lease-create: Creating plans for a platform no backend has starts nothing'] = Get-PlNoStartProblems -StandIn $standIn -Runs @($runs)
  return $results
}

Exit-ConformanceRun

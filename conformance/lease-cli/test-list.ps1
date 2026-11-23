#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check (D34): `agentspace lease list`, its filters, its order and its empty list. It
# runs the program AGENTSPACE_CMD names, as a separate process, in a throwaway home with a throwaway
# CA (conformance/lease-cli/harness.ps1). The leases it lists are made by `lease create`, then given
# the times and the states that docs/state.md lets a state file hold: a ready lease is made `expired`
# by moving its expires_at into the past, and `pending` and `released` records are added to the state
# file by hand, since create answers neither. Checks that need a ready lease are reported SKIPPED
# when no throwaway CA could be made; a run with any SKIPPED is incomplete, never passed.
# The real CA, ~/.ssh and %USERPROFILE%\.agentspace are never touched, and no prototype code is loaded.
# Run: pwsh -NoProfile -File conformance/lease-cli/test-list.ps1
# Exits 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

# A canonical text of a parsed document, with every object's keys sorted, to compare two documents
# whose members come in different orders.
function ConvertTo-CanonicalText {
  param($Value)
  if ($Value -is [System.Collections.IDictionary]) {
    return '{' + (($Value.Keys | Sort-Object -CaseSensitive | ForEach-Object { "$_`:" + (ConvertTo-CanonicalText $Value[$_]) }) -join ',') + '}'
  }
  if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
    return '[' + ((@($Value) | ForEach-Object { ConvertTo-CanonicalText $_ }) -join ',') + ']'
  }
  return ConvertTo-Json -Compress -InputObject $Value
}

# The ids a list run printed, in order, or $null when it printed none.
function Get-ListedIds {
  param($Run)
  if ($null -eq $Run.Json -or $null -eq $Run.Json['leases']) { return $null }
  return , @($Run.Json['leases'] | ForEach-Object { $_['id'] })
}

# Runs list with the arguments and checks it printed exactly the ids expected, in that order.
function Test-ListRun {
  param($ConformanceHome, [string]$Name, [string[]]$Arguments, [string[]]$ExpectIds)
  $r = Invoke-AgentspaceCli -ConformanceHome $ConformanceHome -Arguments (@('lease', 'list') + $Arguments)
  $p = Get-ConformanceSuccessProblems -Run $r -Schema 'lease-list.output.schema.json'
  $got = Get-ListedIds $r
  if ($null -ne $got -and (($got -join ',') -cne ($ExpectIds -join ','))) {
    $p.Add("Printed $($got.Count) leases, ids '$(($got | ForEach-Object { $_.Substring(0, 8) }) -join ',')', not '$(($ExpectIds | ForEach-Object { $_.Substring(0, 8) }) -join ',')'")
  }
  if ($ExpectIds.Count -eq 0 -and $r.Stdout -cne "{`"leases`":[]}`n") { $p.Add('An empty list is not exactly {"leases":[]} and a line feed') }
  Complete-ConformanceCheck -Name $Name -Problems $p
}

$h = $null
try {
  $h = New-ConformanceHome

  # --- No leases at all (lease-api.md#agentspace-lease-list; state.md#the-lease-state-file) ---------
  Test-ListRun -ConformanceHome $h -Arguments @() -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: With no leases kept, list prints exactly {"leases":[]} and exits 0'
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'x', '--state', 'ready') -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: With no leases kept, list with filters prints an empty list too'
  $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'list')
  $p = [System.Collections.Generic.List[string]]::new()
  if ($r.ExitCode -ne 0 -or $r.Stdout -cne "{`"leases`":[]}`n") { $p.Add('list did not answer an empty list, so there is nothing to compare') }
  if (Test-Path -LiteralPath (Get-ConformanceStatePath -ConformanceHome $h)) { $p.Add('list made a state file') }
  Complete-ConformanceCheck -Name 'state.md#the-lease-state-file: list never writes the state file, so a missing file stays missing' -Problems $p

  # --- Seeds: made by create, then given times and states ------------------------------------------
  $seeds = [System.Collections.Generic.List[object]]::new()   # Each: id, project, state, created_at
  $haveReady = [bool]$h.CaReady
  function New-Seed { param([string]$Project, [string[]]$Extra = @())
    $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments (@('lease', 'create', '--project', $Project) + $Extra)
    if ($r.ExitCode -ne 0 -or -not $r.Json) { throw "A seed create failed: exit $($r.ExitCode)" }
    return $r.Json
  }
  $r1 = $null; $r2 = $null
  # Under D54 a create is ready only for a project with a ready host (E0.07-T04-S01). The three
  # unsatisfiable seeds below are unsupported requests, which never reach the host registry.
  # The hosts are needed for these two creates alone: once they are made the registry is removed again,
  # because a `get` of the pending alpha lease below would start a provisioning run for a project whose
  # host is ready (D48), and no case here is about that.
  if ($haveReady) {
    [void](Add-ConformanceReadyHost -ConformanceHome $h -Project 'alpha', 'beta')
    $r1 = New-Seed 'alpha'; $r2 = New-Seed 'beta'
    Clear-ConformanceHosts -ConformanceHome $h
  }
  $u1 = New-Seed 'alpha' @('--size', 'xxl')
  $u2 = New-Seed 'beta' @('--size', 'xxl')
  $u3 = New-Seed 'alpha' @('--platform', 'linux/amd64')

  $doc = Read-ConformanceStateFile -ConformanceHome $h
  $byId = @{}
  foreach ($rec in $doc.leases) { $byId[$rec.id] = $rec }
  $clone = { param($rec) ($rec | ConvertTo-Json -Depth 50 | ConvertFrom-Json -DateKind String) }
  $p1 = & $clone $byId[$u1['id']]          # pending: the common fields, and none of expires_at, released_at, ssh and reasons
  $p1.id = 'ffffffff-ffff-4fff-bfff-ffffffffffff'; $p1.state = 'pending'; $p1.PSObject.Properties.Remove('reasons')
  $rel = & $clone $byId[$u2['id']]         # released from pending: the common fields and released_at
  $rel.id = '00000000-0000-4000-8000-000000000001'; $rel.state = 'released'; $rel.PSObject.Properties.Remove('reasons')
  $rel | Add-Member -NotePropertyName released_at -NotePropertyValue '2026-11-18T10:30:00Z'

  # created_at: not in file order, with two ties that only the id can break. Rel1's id sorts before u2's
  # though it is written after it, and the pending record's sorts after everything.
  $stamp = @{}
  if ($haveReady) { $stamp[$r1['id']] = '2026-11-18T10:00:05Z'; $stamp[$r2['id']] = '2026-11-18T09:59:59Z' }
  $stamp[$u1['id']] = '2026-11-18T10:00:05Z'; $stamp[$u2['id']] = '2026-11-18T10:00:03Z'; $stamp[$u3['id']] = '2026-11-18T10:00:01Z'
  $stamp[$p1.id] = '2026-11-18T10:00:01Z'; $stamp[$rel.id] = '2026-11-18T10:00:03Z'
  foreach ($rec in $doc.leases) { $rec.created_at = $stamp[$rec.id] }
  $p1.created_at = $stamp[$p1.id]; $rel.created_at = $stamp[$rel.id]
  if ($haveReady) { $rec = $byId[$r2['id']]; $rec.expires_at = '2020-01-01T00:00:00Z' }   # r2 reads as expired
  $doc.leases = @($doc.leases) + $p1 + $rel
  Write-ConformanceStateFile -ConformanceHome $h -Document $doc

  if ($haveReady) {
    $seeds.Add(@{ id = $r1['id']; project = 'alpha'; state = 'ready'; created_at = $stamp[$r1['id']] })
    $seeds.Add(@{ id = $r2['id']; project = 'beta'; state = 'expired'; created_at = $stamp[$r2['id']] })
  }
  $seeds.Add(@{ id = $u1['id']; project = 'alpha'; state = 'unsatisfiable'; created_at = $stamp[$u1['id']] })
  $seeds.Add(@{ id = $u2['id']; project = 'beta'; state = 'unsatisfiable'; created_at = $stamp[$u2['id']] })
  $seeds.Add(@{ id = $u3['id']; project = 'alpha'; state = 'unsatisfiable'; created_at = $stamp[$u3['id']] })
  $seeds.Add(@{ id = $p1.id; project = 'alpha'; state = 'pending'; created_at = $stamp[$p1.id] })
  $seeds.Add(@{ id = $rel.id; project = 'beta'; state = 'released'; created_at = $stamp[$rel.id] })

  # The order lease-api.md#agentspace-lease-list requires: created_at, then id, both ascending, compared
  # character by character.
  $ordered = [System.Collections.Generic.List[object]]::new($seeds)
  $ordered.Sort([Comparison[object]] {
      param($x, $y)
      $c = [string]::CompareOrdinal($x.created_at, $y.created_at)
      if ($c -ne 0) { return $c }
      return [string]::CompareOrdinal($x.id, $y.id)
    })
  # The ids a filter keeps, in the required order.
  $pick = { param([scriptblock]$Where) @($ordered | Where-Object $Where | ForEach-Object { $_.id }) }
  $stateHash = Get-ConformanceStateHash -ConformanceHome $h

  # --- Every lease, in order ----------------------------------------------------------------------
  Test-ListRun -ConformanceHome $h -Arguments @() -ExpectIds (& $pick { $true }) `
    -Name 'lease-api.md#agentspace-lease-list: list prints every kept lease, oldest first by created_at and then id, and validates against lease-list.output.schema.json'

  $all = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'list')
  $p = [System.Collections.Generic.List[string]]::new()
  $items = if ($all.Json -and $all.Json['leases']) { @($all.Json['leases']) } else { @() }
  if ($items.Count -lt $seeds.Count) { $p.Add("The list holds $($items.Count) leases, fewer than the $($seeds.Count) seeded") }
  foreach ($item in $items) {
    $g = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', [string]$item['id'])
    if ($null -eq $g.Json -or (ConvertTo-CanonicalText $g.Json) -cne (ConvertTo-CanonicalText $item)) { $p.Add("The entry for $($item['id'].Substring(0, 8)) is not what get prints") }
  }
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-list: Each lease in the list is the document get prints for it' -Problems $p

  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($seed in $seeds) {
    $item = @($all.Json['leases'] | Where-Object { $_['id'] -ceq $seed.id })[0]
    if ($null -eq $item) { $p.Add("$($seed.id.Substring(0, 8)) is missing") }
    elseif ($item['state'] -cne $seed.state) { $p.Add("$($seed.id.Substring(0, 8)) is $($item['state']), not $($seed.state)") }
  }
  Complete-ConformanceCheck -Name 'lease-api.md#states: list shows each lease in the state get decides: ready, expired, pending, released and unsatisfiable' -Problems $p

  # --- The project filter, matched on the normalized name -------------------------------------------
  foreach ($project in 'alpha', 'beta') {
    Test-ListRun -ConformanceHome $h -Arguments @('--project', $project) -ExpectIds (& $pick ([scriptblock]::Create("`$_.project -ceq '$project'"))) `
      -Name "lease-api.md#agentspace-lease-list: list --project $project prints only that project's leases, in order"
  }
  Test-ListRun -ConformanceHome $h -Arguments @('--project', ' ALPHA ') -ExpectIds (& $pick { $_.project -ceq 'alpha' }) `
    -Name 'lease-api.md#agentspace-lease-list: list --project '' ALPHA '' matches the normalized name alpha'
  Test-ListRun -ConformanceHome $h -Arguments @('--project=Beta') -ExpectIds (& $pick { $_.project -ceq 'beta' }) `
    -Name 'lease-api.md#agentspace-lease-list: list --project=Beta matches the normalized name beta'
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'gamma') -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: A project with no lease is not an error: {"leases":[]}, exit 0'
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'alph') -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: The project filter matches the whole name, never a part of it'

  # --- The state filter ---------------------------------------------------------------------------
  $stateCases = @(
    @{ Args = @('--state', 'unsatisfiable'); States = @('unsatisfiable') }
    @{ Args = @('--state', 'pending'); States = @('pending') }
    @{ Args = @('--state', 'released'); States = @('released') }
    @{ Args = @('--state', 'pending,released'); States = @('pending', 'released') }
    @{ Args = @('--state', 'released', '--state', 'pending'); States = @('pending', 'released') }
    @{ Args = @('--state', ' Pending , RELEASED ,pending'); States = @('pending', 'released') }
    @{ Args = @('--state=pending,unsatisfiable'); States = @('pending', 'unsatisfiable') }
    @{ Args = @('--STATE', 'unsatisfiable', '--state', 'unsatisfiable'); States = @('unsatisfiable') }
  )
  if ($haveReady) {
    $stateCases += @(
      @{ Args = @('--state', 'ready'); States = @('ready') }
      @{ Args = @('--state', 'expired'); States = @('expired') }
      @{ Args = @('--state', 'ready,expired'); States = @('ready', 'expired') }
      @{ Args = @('--state', 'ready', '--state', 'expired'); States = @('ready', 'expired') }
      @{ Args = @('--state', ' Ready , EXPIRED '); States = @('ready', 'expired') }
      @{ Args = @('--state', 'ready,pending,released,unsatisfiable'); States = @('ready', 'pending', 'released', 'unsatisfiable') }
      @{ Args = @('--state', 'ready,ready'); States = @('ready') }
    )
  }
  foreach ($case in $stateCases) {
    $states = $case.States
    Test-ListRun -ConformanceHome $h -Arguments $case.Args -ExpectIds (& $pick { $states -ccontains $_.state }) `
      -Name "lease-api.md#state-filter: $(Format-ConformanceLine (@('lease', 'list') + $case.Args)) prints the leases in $($states -join ', ')"
  }
  if (-not $haveReady) {
    Write-ConformanceResult -Name 'lease-api.md#states: list --state ready leaves out an expired lease, and --state expired includes it' -Status SKIPPED -Reason ('No throwaway CA could be made: ' + $h.CaReason)
  }
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'alpha', '--state', 'unsatisfiable') -ExpectIds (& $pick { $_.project -ceq 'alpha' -and $_.state -ceq 'unsatisfiable' }) `
    -Name 'lease-api.md#agentspace-lease-list: The project and state filters both apply'
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'beta', '--state', 'pending') -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: Filters that match nothing give {"leases":[]}, exit 0'
  Test-ListRun -ConformanceHome $h -Arguments @('--project', 'gamma', '--state', 'ready,pending') -ExpectIds @() `
    -Name 'lease-api.md#agentspace-lease-list: A project with no lease and a state list gives {"leases":[]}, exit 0'

  # Command and flags in any case, as everywhere (lease-api.md#command-lines).
  $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('LEASE', 'LIST', '--PROJECT=alpha', '--STATE', 'unsatisfiable')
  $p = Get-ConformanceSuccessProblems -Run $r -Schema 'lease-list.output.schema.json'
  $want = & $pick { $_.project -ceq 'alpha' -and $_.state -ceq 'unsatisfiable' }
  $gotIds = Get-ListedIds $r
  if ((@($gotIds) -join ',') -cne ($want -join ',')) { $p.Add('The leases printed are not alpha''s unsatisfiable ones') }
  Complete-ConformanceCheck -Name 'lease-api.md#command-lines: agentspace LEASE LIST --PROJECT=alpha --STATE unsatisfiable is read as lease list --project=alpha --state unsatisfiable' -Problems $p

  # list changes nothing (lease-api.md#agentspace-lease-list; state.md#the-lease-state-file).
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-ConformanceStateHash -ConformanceHome $h) -cne $stateHash) { $p.Add('The state file changed') }
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-list: list changes nothing, so the state file is byte for byte as it was, expired leases included' -Problems $p
} catch {
  Write-ConformanceResult -Name 'test-list.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h) { Remove-ConformanceHome -ConformanceHome $h }
}
Exit-ConformanceRun

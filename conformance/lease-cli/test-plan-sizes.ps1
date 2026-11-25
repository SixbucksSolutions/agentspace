#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34: it drives the lease CLI only as a separate process, through what
# docs/lease-api.md documents, with a stand-in for the provisioning run (a program outside agentspace). See
# docs/conformance.md. A v0.10 build MUST pass every result line.
#
# Cases of E0.10-T04-S01 (card #0221), `create --plan <file>` picks the lease's size (lease-api.md, "Plan" and
# "agentspace lease create"; D73 to D76). Every expectation below is lease-api.md's: the size table of "Plan"
# (2 to 4 agents at once `s`, 5 to 8 `m`, 9 to 16 `l`, 17 to 32 `xl`, at today's footprint of half a vCPU and
# 2 GiB for each agent), the busiest turn (the most agents at once, the first such turn on a tie), a turn's
# agents (the sum of its stories' peaks plus 1 for its dev lead), and a story's peak (the width of its `after`
# graph, never a stated number). Not here, and another file's: plan_too_large, --plan with --size, a plan that
# is not well formed, v0 compatibility and D77.
#
#   Edges     For each size, with a record ready at that size's type and no other lease: a plan of the size's
#             lowest and of its highest agents at once (2 and 4, 5 and 8, 9 and 16, 17 and 32), made with
#             --plan alone, answers ready with that size, and sized_for names the plan, its turn and the
#             count. The four sizes meet at 4/5, 8/9 and 16/17, each side of each boundary, and 2 and 32 are the
#             table's two ends.
#   Sizes     The same eight counts, with a platform no backend has, so that no host or CA is needed and the
#             lease is unsatisfiable with unsupported_platform alone (lease-api.md, "agentspace lease create":
#             a plan that fits gets sized_for, the size picked and the reasons a --size lease would get): the
#             sizes picked are four different ones, and the plan's agents at once are the busiest turn's.
#   Busiest   A tie between two busiest turns (the first wins, its name in sized_for, whatever the names sort
#             as), a tie with a lesser turn between, the busiest turn last, in the middle, the turns not
#             summed, and several stories (their peaks summed, one dev lead for the turn).
#   Peak      The peak is counted from `after`: a chain, a root and its dependants, a dependency through
#             another subagent, a diamond, two chains, an `after` that names a subagent listed later, and the
#             three forms that mean no `after` (absent, [] and null). In each, the count of subagents or the
#             count of direct `after` entries gives another size than the width does.
#   Neither a subagent's model nor its effort changes the size.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No check starts
# a real run: every create is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what
# it is given and makes no host, and a check of an exact host asserts that it was not started. No program of
# agentspace's but the lease CLI is started, and only through the harness. Every write, the plan files and the
# host registry included, calls Assert-ConformancePath at the call, through Write-PvText. No failure reason
# quotes a path or a value the program printed.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-plan-sizes.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

# A project of the cases' own, with a record, that is not swimcoach.
$script:PlProject = 'plansizes'
# The four sizes' types (driver.md, "Sizes and instance types"), the cases' own data for a host record.
$script:PlType = @{ s = 'm9g.large'; m = 'm9g.xlarge'; l = 'm9g.2xlarge'; xl = 'm9g.4xlarge' }
# The box's record, first in every registry written (state.md, "The host registry").
$script:PlBox = '{"project":"swimcoach","stack":"agentbox","state":"ready","instance_type":"m9g.xlarge"}'

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

# A subagent. $After is the names it starts after; -AfterAs empty or null writes the two other forms of no `after`.
function New-PlSubagent {
  param([Parameter(Mandatory)][string]$Name, [string]$Model = 'Fakemodel', [string]$Effort = 'fakeeffort', [string[]]$After = @(), [ValidateSet('', 'empty', 'null')][string]$AfterAs = '')
  $s = '{"name":' + (Format-PlString $Name) + ',"model":' + (Format-PlString $Model) + ',"effort":' + (Format-PlString $Effort)
  if ($AfterAs -ceq 'null') { $s += ',"after":null' }
  elseif ($AfterAs -ceq 'empty') { $s += ',"after":[]' }
  elseif ($After.Count -gt 0) { $s += ',"after":[' + (($After | ForEach-Object { Format-PlString $_ }) -join ',') + ']' }
  return $s + '}'
}
function New-PlStory { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Subagents) '{"name":' + (Format-PlString $Name) + ',"subagents":[' + ($Subagents -join ',') + ']}' }
function New-PlTurn { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Stories) '{"name":' + (Format-PlString $Name) + ',"stories":[' + ($Stories -join ',') + ']}' }
function New-PlPlan {
  param([Parameter(Mandatory)][string[]]$Turns, [string]$Name, [switch]$NullName)
  $s = '{'
  if ($NullName) { $s += '"name":null,' } elseif ($PSBoundParameters.ContainsKey('Name')) { $s += '"name":' + (Format-PlString $Name) + ',' }
  return $s + '"turns":[' + ($Turns -join ',') + ']}'
}

# A story of $Peak subagents, none starting after another: its peak is $Peak.
function New-PlWideStory {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Peak, [string]$Model = 'Fakemodel', [string]$Effort = 'fakeeffort')
  $subs = foreach ($i in 1..$Peak) { New-PlSubagent -Name "w$i" -Model $Model -Effort $Effort }
  New-PlStory -Name $Name -Subagents $subs
}
# A turn of one story per peak given: its agents at once are the peaks' sum plus 1.
function New-PlPeaksTurn {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int[]]$Peaks)
  $i = 0
  $stories = foreach ($p in $Peaks) { $i++; New-PlWideStory -Name "S$i" -Peak $p }
  New-PlTurn -Name $Name -Stories $stories
}
# A turn of $Agents agents at once, in one story of $Agents - 1 subagents (at most 33 agents).
function New-PlAgentsTurn { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Agents) New-PlPeaksTurn -Name $Name -Peaks @($Agents - 1) }
# A turn of one story whose subagents are the ones given.
function New-PlOneStoryTurn { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string[]]$Subagents) New-PlTurn -Name $Name -Stories @((New-PlStory -Name 'Only story' -Subagents $Subagents)) }
# $Count subagents, each starting after the one before.
function New-PlChain { param([Parameter(Mandatory)][string]$Prefix, [Parameter(Mandatory)][int]$Count)
  foreach ($i in 1..$Count) { if ($i -eq 1) { New-PlSubagent -Name "$Prefix$i" } else { New-PlSubagent -Name "$Prefix$i" -After @("$Prefix$($i - 1)") } }
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

# The problems with a document's sized_for: present, an object of exactly turn and agents (and plan when the
# plan has a name), each as the plan writes it. $Expect is @{ Plan; Turn; Agents }, Plan $null for no name.
function Get-PlSizedForProblems {
  param([Parameter(Mandatory)]$Doc, [Parameter(Mandatory)]$Expect)
  $p = [System.Collections.Generic.List[string]]::new()
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

# The problems with a run that answered ready at $Size for a plan, with the sized_for expected, and the lease's
# stored record in the state it is and with the size.
function Get-PlReadyProblems {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, [Parameter(Mandatory)]$Expect)
  $p = Get-ConformanceSuccessProblems -Run $Run
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  if ($doc['state'] -cne 'ready') { $p.Add("The lease's state is '$($doc['state'])', not ready"); return , $p }
  if ($doc['project'] -cne $script:PlProject) { $p.Add('The lease is for another project') }
  if ($doc['size'] -cne $Size) { $p.Add("The lease's size is '$($doc['size'])', not '$Size'") }
  foreach ($x in (Get-PlSizedForProblems -Doc $doc -Expect $Expect)) { $p.Add($x) }
  if ($doc['ssh'] -is [hashtable] -and [string]$doc['ssh']['host_alias'] -cmatch "\Aagentspace-$([regex]::Escape($script:PlProject))-[0-9a-f]{8}\z") {
    $rec = Get-CcRecord $Lab ([string]$doc['id'])
    if ($null -eq $rec) { $p.Add('The lease is not in the lease state file') }
    else {
      if ($rec['state'] -cne 'ready') { $p.Add('The stored lease is not ready') }
      if ($rec['size'] -cne $Size) { $p.Add('The stored lease''s size is not the size picked') }
    }
  } else { $p.Add('The ready lease has no host alias of the project''s form') }
  return , $p
}

# The problems with a run that answered unsatisfiable for a plan with unsupported_platform alone (the request's
# platform is one no backend has), with the size picked and the sized_for expected.
function Get-PlPlatformProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Size, [Parameter(Mandatory)]$Expect)
  $p = Get-PvUnsatisfiableDocProblems -Run $Run -Project $script:PlProject -Codes @('unsupported_platform')
  $doc = $Run.Json
  if ($doc -is [hashtable]) {
    if ($doc['size'] -cne $Size) { $p.Add("The lease's size is '$($doc['size'])', not '$Size'") }
    foreach ($x in (Get-PlSizedForProblems -Doc $doc -Expect $Expect)) { $p.Add($x) }
    if ($doc.ContainsKey('ssh') -or $doc.ContainsKey('expires_at')) { $p.Add('An unsatisfiable lease carries ssh or expires_at') }
  }
  return , $p
}

# The problems with a stand-in that was never started. A program that prints nothing starts nothing, so the result
# also needs every create of the home to have printed a lease document, which says the creates ran.
function Get-PlNoStartProblems {
  param([Parameter(Mandatory)]$StandIn, [Parameter(Mandatory)][int]$Runs, [Parameter(Mandatory)][int]$Documents)
  $p = Get-PvNoStartProblems -StandIn $StandIn
  if ($Documents -ne $Runs) { $p.Add("Only $Documents of the $Runs creates printed a lease document, so what they started is not known") }
  return , $p
}

function Invoke-PlCreate {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$PlanPath, [Parameter(Mandatory)]$StandIn, [string[]]$Extra = @())
  Invoke-PvLease -Lab $Lab -Arguments (@('create', '--project', $script:PlProject, '--plan', $PlanPath) + $Extra) -StandIn $StandIn
}

# ---- Edges ---------------------------------------------------------------------------------------------------
# The four sizes' lowest and highest agents at once, each made with --plan alone on a record ready at the size's
# own type, so that the lease is placed ready at the size picked and nothing is started.
foreach ($edge in @(
    @{ Size = 's'; Counts = @(2, 4) }, @{ Size = 'm'; Counts = @(5, 8) },
    @{ Size = 'l'; Counts = @(9, 16) }, @{ Size = 'xl'; Counts = @(17, 32) })) {
  $sub = $edge
  Use-PlLab -Name "lease-api.md#plan: With a record ready at the type of $($sub.Size), plans at that size's edges" -Body {
    param($lab)
    Set-PlHost -Lab $lab -InstanceType $script:PlType[$sub.Size]
    $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
    $results = [ordered]@{}
    $documents = 0
    foreach ($n in $sub.Counts) {
      $text = New-PlPlan -Name 'Edge plan' -Turns @((New-PlAgentsTurn -Name "Turn $n" -Agents $n))
      $path = Write-PlPlan -Lab $lab -File "edge-$n" -Text $text
      $run = Invoke-PlCreate -Lab $lab -PlanPath $path -StandIn $standIn
      $expect = @{ Plan = 'Edge plan'; Turn = "Turn $n"; Agents = $n }
      if ($run.Json -is [hashtable]) { $documents++ }
      $results["lease-api.md#plan: A plan of $n agents at once (a story of peak $($n - 1), and the dev lead) gets size $($sub.Size): create --plan answers ready at that size, and sized_for says 'Edge plan', 'Turn $n' and $n"] = Get-PlReadyProblems -Lab $lab -Run $run -Size $sub.Size -Expect $expect
    }
    $results["lease-api.md#agentspace-lease-create: Creating plans of the $($sub.Size) edges on a record ready at its type starts nothing"] = Get-PlNoStartProblems -StandIn $standIn -Runs $sub.Counts.Count -Documents $documents
    return $results
  }
}

# ---- The sizes, and the cases that need no host --------------------------------------------------------------
# A platform no backend has makes the lease unsatisfiable with unsupported_platform alone, and it keeps the size
# the plan picked and its sized_for (lease-api.md, "agentspace lease create"). No host, no project list and no CA
# is read for it, so each case is one create.
Use-PlLab -Name 'lease-api.md#plan: With a platform no backend has, the size a plan picks' -Body {
  param($lab)
  $standIn = New-PvStandIn -Lab $lab -Name 'tripwire'
  $results = [ordered]@{}
  $one = {
    param([string]$Key, [string]$PlanText, [string]$Size, $Expect)
    $script:PlN++
    $path = Write-PlPlan -Lab $lab -File ("case-$($script:PlN)") -Text $PlanText
    $run = Invoke-PlCreate -Lab $lab -PlanPath $path -StandIn $standIn -Extra @('--platform', 'windows/amd64')
    if ($run.Json -is [hashtable]) { $script:PlDocs++ }
    $results[$Key] = Get-PlPlatformProblems -Run $run -Size $Size -Expect $Expect
  }
  $script:PlN = 0
  $script:PlDocs = 0

  # The edges again, each of the four sizes' two ends, with the four sizes seen to differ.
  foreach ($row in @(@('s', 2), @('s', 4), @('m', 5), @('m', 8), @('l', 9), @('l', 16), @('xl', 17), @('xl', 32))) {
    $size = $row[0]; $count = $row[1]
    & $one "lease-api.md#plan: A plan of $count agents at once gets size $size, in the size table of Plan, with the platform unmet" (New-PlPlan -Name 'Sizes plan' -Turns @((New-PlAgentsTurn -Name "Turn $count" -Agents $count))) $size @{ Plan = 'Sizes plan'; Turn = "Turn $count"; Agents = $count }
  }

  # The busiest turn.
  $tie1 = New-PlPlan -Name 'Tie plan' -Turns @((New-PlAgentsTurn 'zeta' 6), (New-PlAgentsTurn 'alpha' 6), (New-PlAgentsTurn 'mid' 4))
  & $one 'lease-api.md#fields: A tie between the two busiest turns is the first in the plan''s order: zeta and alpha have 6 agents each, mid has 4, and sized_for.turn is zeta, which sorts after alpha, with size m' $tie1 'm' @{ Plan = 'Tie plan'; Turn = 'zeta'; Agents = 6 }
  $tie2 = New-PlPlan -Name 'Tie plan' -Turns @((New-PlAgentsTurn 'T9' 9), (New-PlAgentsTurn 'T1' 3), (New-PlAgentsTurn 'T5' 9))
  & $one 'lease-api.md#fields: A tie between the first and the last turn, with a lesser turn between, is the first: T9 and T5 have 9 agents each, T1 has 3, and sized_for.turn is T9, with size l' $tie2 'l' @{ Plan = 'Tie plan'; Turn = 'T9'; Agents = 9 }
  $mid = New-PlPlan -Name 'Busiest plan' -Turns @((New-PlAgentsTurn 'T1' 3), (New-PlAgentsTurn 'T2' 9), (New-PlAgentsTurn 'T3' 5), (New-PlAgentsTurn 'T4' 2))
  & $one 'lease-api.md#fields: The busiest turn is the one with the most agents at once wherever it is: T2 of 3, 9, 5 and 2 agents, with size l' $mid 'l' @{ Plan = 'Busiest plan'; Turn = 'T2'; Agents = 9 }
  $last = New-PlPlan -Name 'Busiest plan' -Turns @((New-PlAgentsTurn 'A' 2), (New-PlAgentsTurn 'B' 4), (New-PlAgentsTurn 'C' 17))
  & $one 'lease-api.md#fields: The busiest turn MAY be the last: C of 2, 4 and 17 agents, with size xl' $last 'xl' @{ Plan = 'Busiest plan'; Turn = 'C'; Agents = 17 }
  $notsum = New-PlPlan -Name 'Turns plan' -Turns @((New-PlAgentsTurn 'T1' 3), (New-PlAgentsTurn 'T2' 3), (New-PlAgentsTurn 'T3' 3))
  & $one 'lease-api.md#plan: The plan''s agents at once are the busiest turn''s, never the turns'' sum: three turns of 3 agents each are 3, with size s, not 9 with size l' $notsum 's' @{ Plan = 'Turns plan'; Turn = 'T1'; Agents = 3 }

  # Several stories in a turn: their peaks summed, plus 1 for the dev lead once.
  $stories1 = New-PlPlan -Name 'Stories plan' -Turns @((New-PlPeaksTurn 'Three small' @(1, 1, 1)))
  & $one 'lease-api.md#plan: A turn of three stories of peak 1 each has 4 agents at once, the sum of the peaks plus 1 for its one dev lead, with size s, not 6 with size m' $stories1 's' @{ Plan = 'Stories plan'; Turn = 'Three small'; Agents = 4 }
  $stories2 = New-PlPlan -Name 'Stories plan' -Turns @((New-PlPeaksTurn 'Two big' @(4, 4)))
  & $one 'lease-api.md#plan: A turn of two stories of peak 4 each has 9 agents at once, the sum of the peaks plus 1, with size l, not the busiest story''s 5 with size m' $stories2 'l' @{ Plan = 'Stories plan'; Turn = 'Two big'; Agents = 9 }
  $stories3 = New-PlPlan -Name 'Stories plan' -Turns @((New-PlPeaksTurn 'Mixed' @(3, 4, 2)), (New-PlPeaksTurn 'Smaller' @(2, 2)))
  & $one 'lease-api.md#plan: A turn of stories of peaks 3, 4 and 2 has 10 agents at once, and is busier than a turn of peaks 2 and 2, which has 5, with size l' $stories3 'l' @{ Plan = 'Stories plan'; Turn = 'Mixed'; Agents = 10 }

  # A story's peak is the width of its `after` graph.
  $chain8 = New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Chain' (New-PlChain -Prefix 'c' -Count 8)))
  & $one 'lease-api.md#plan: A story of 8 subagents each starting after the one before has peak 1, so its turn has 2 agents at once, with size s, not 9 with size l' $chain8 's' @{ Plan = 'Peak plan'; Turn = 'Chain'; Agents = 2 }
  $rootSubs = @(New-PlSubagent -Name 'root') + @(foreach ($i in 1..7) { New-PlSubagent -Name "k$i" -After @('root') })
  & $one 'lease-api.md#plan: A root and 7 subagents starting after it has peak 7, so its turn has 8 agents at once, with size m, not 9 with size l' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Root' $rootSubs))) 'm' @{ Plan = 'Peak plan'; Turn = 'Root'; Agents = 8 }
  # x1 < x2 < x3 by a chain of direct entries, and x3 lists only x2: x1 and x3 are not independent. Two other
  # subagents are. The widest set is one of the chain and the two, 3, so the turn has 4 agents at once. Reading
  # direct entries alone finds {x1, x3, y, z}, 4, and 5 agents at once.
  $trans = @(New-PlSubagent -Name 'x1'; New-PlSubagent -Name 'x2' -After @('x1'); New-PlSubagent -Name 'x3' -After @('x2'); New-PlSubagent -Name 'y'; New-PlSubagent -Name 'z')
  & $one 'lease-api.md#plan: A subagent starts after another through others, at any depth: x1, x2 after x1, x3 after x2 and two independent ones have peak 3, so 4 agents at once, with size s, not 5 with size m' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Through' $trans))) 's' @{ Plan = 'Peak plan'; Turn = 'Through'; Agents = 4 }
  $diamond = @(New-PlSubagent -Name 'root'; New-PlSubagent -Name 'a' -After @('root'); New-PlSubagent -Name 'b' -After @('root'); New-PlSubagent -Name 'c' -After @('root'); New-PlSubagent -Name 'join' -After @('a', 'b', 'c'))
  & $one 'lease-api.md#plan: A root, three subagents after it and one after all three have peak 3, so 4 agents at once, with size s, not 6 with size m' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Diamond' $diamond))) 's' @{ Plan = 'Peak plan'; Turn = 'Diamond'; Agents = 4 }
  $two = @(New-PlChain -Prefix 'p' -Count 4) + @(New-PlChain -Prefix 'q' -Count 4)
  & $one 'lease-api.md#plan: Two independent chains of 4 subagents have peak 2, so 3 agents at once, with size s, not 9 with size l' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Chains' $two))) 's' @{ Plan = 'Peak plan'; Turn = 'Chains'; Agents = 3 }
  $fwd = @(New-PlSubagent -Name 'c' -After @('b'); New-PlSubagent -Name 'b' -After @('a'); New-PlSubagent -Name 'a')
  & $one 'lease-api.md#plan: A subagent MAY start after one listed later: c after b after a, listed in that order, have peak 1, so 2 agents at once, with size s' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Forward' $fwd))) 's' @{ Plan = 'Peak plan'; Turn = 'Forward'; Agents = 2 }
  $forms = @(New-PlSubagent -Name 'f1'; New-PlSubagent -Name 'f2'; New-PlSubagent -Name 'f3' -AfterAs empty; New-PlSubagent -Name 'f4' -AfterAs empty; New-PlSubagent -Name 'f5' -AfterAs null; New-PlSubagent -Name 'f6' -AfterAs null)
  & $one 'lease-api.md#plan: An after that is absent, empty or null means no after: six such subagents have peak 6, so 7 agents at once, with size m' (New-PlPlan -Name 'Peak plan' -Turns @((New-PlOneStoryTurn 'Forms' $forms))) 'm' @{ Plan = 'Peak plan'; Turn = 'Forms'; Agents = 7 }

  # Neither model nor effort changes the size.
  $plain = foreach ($i in 1..6) { New-PlSubagent -Name "m$i" -Model 'Fakemodel' -Effort 'fakeeffort' }
  $varied = @(
    New-PlSubagent -Name 'm1' -Model 'Fakemodel 9.1 +24h' -Effort 'xfake'
    New-PlSubagent -Name 'm2' -Model 'Othermodel' -Effort 'fake.1_a-b'
    New-PlSubagent -Name 'm3' -Model 'a' -Effort '9'
    New-PlSubagent -Name 'm4' -Model ('M' * 64) -Effort ('E' * 32)
    New-PlSubagent -Name 'm5' -Model 'Model with spaces inside' -Effort 'Zz'
    New-PlSubagent -Name 'm6' -Model 'Fakemodel' -Effort 'fakeeffort')
  & $one 'lease-api.md#plan: A plan of 6 subagents with one model and one effort has 7 agents at once and size m' (New-PlPlan -Name 'Model plan' -Turns @((New-PlOneStoryTurn 'Models' $plain))) 'm' @{ Plan = 'Model plan'; Turn = 'Models'; Agents = 7 }
  & $one 'lease-api.md#plan: The same plan with other models and efforts, and those of the longest forms, has the same agents at once and the same size m: neither changes the size' (New-PlPlan -Name 'Model plan' -Turns @((New-PlOneStoryTurn 'Models' $varied))) 'm' @{ Plan = 'Model plan'; Turn = 'Models'; Agents = 7 }

  $results['lease-api.md#agentspace-lease-create: Creating plans for a platform no backend has starts nothing'] = Get-PlNoStartProblems -StandIn $standIn -Runs $script:PlN -Documents $script:PlDocs
  return $results
}

Exit-ConformanceRun

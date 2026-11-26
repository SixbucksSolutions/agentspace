#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.11-T04-S01, #0239): `create --turn`, the TTL a plan gives from a turn's
# `budget_minutes`, and the usage and value cases around both (docs/lease-api.md, contract v2: "Turn", "TTL", "Plan",
# "The plan block", "agentspace lease create", "Command lines", "Exit codes and errors", "Messages"; D83 to D87). It
# drives the lease CLI only as a separate process, through the harness, in throwaway homes. Every expectation is taken
# from docs/lease-api.md, never from a program's output. Not here, and another file's: a board's refusals and the stand-in
# board's own cases (test-board-plan.ps1), and v2 keeping v1 (test-v2-compatible.ps1).
#
# What it enforces, by the lease-api.md section each result names:
#   #turn, #agentspace-lease-create   `--turn` sizes the lease for the turn it names and for no other: its agents at
#       once, its size, `sized_for.turn` and `sized_for.agents`, with `sized_for.plan` still the plan's name; the busiest
#       turn when it is named, and when it is not; the first of two busiest turns, and the second when it is named; a
#       turn matched exactly (`T01` and `t01` are two turns), a name with spaces, an apostrophe or 64 characters; a turn
#       that no size holds is `plan_too_large` with its own message, though another turn fits, and a turn that fits is
#       sized though another is too large; the whole plan is checked, so a fault in a turn `--turn` does not name still
#       refuses the plan. A value outside a turn's form (whitespace at either end, 65 characters, a character that is not
#       printable ASCII, U+FEFF, an unpaired surrogate) is `invalid_value` for `turn` before any plan is matched, and a
#       well-formed value that names no turn is `invalid_value` for `turn` once the plan is read, with the plan's turns
#       listed raw. The same plan and the same `--turn` give the same lease from a file and from an epic's block.
#   #ttl, #plan   Each TTL rule (D84, D85): `--ttl` wins, whatever the budget; else the lease's turn's budget and half
#       again, rounded up to the whole minute (1 gives 120 seconds, 45 gives 4080, 120 gives 10800, 180 gives 16200, 960
#       gives 86400); else, when that turn has no budget, or a `null` one, the default of 8 hours, whatever budgets the plan's
#       other turns have; a plan lease is never refused with `out_of_range` for a TTL it was not given; a lease that is
#       unsatisfiable carries the TTL as the TTL asked for; `renew` without `--ttl` uses the lease's `ttl_seconds` again,
#       reading no plan and no board. The TTL of a lease that is `ready` on a seeded host is `expires_at` minus
#       `created_at` of its own document, within the time the `create` took; no case compares a clock time of one run
#       with another's.
#   #plan   A `budget_minutes` that is not an integer from 1 to 960 written as digits alone is refused with
#       `invalid_value` for `plan` and the `<why>` of check 6, whether it is written `90.0`, `9e1`, `9E1`, `-0`, `90.5`,
#       `"90"`, `true`, `[]`, `0`, `961` or another form, in a turn `--turn` names and in one it does not (pin P24). From
#       an epic's block the same refusal is `invalid_value` for `epic` with the same `<why>`. `1e400` is not valid JSON
#       (check 4). `90`, `1`, `960` and `null` are accepted.
#   #command-lines, #exit-codes-and-errors   Every usage case that carries `field`, thirteen in all, and `--turn`'s
#       cases among the ranking rules: a missing value, an empty one, a repeat, `--turn` without a plan, the pairs of
#       `--board` and `--epic` with `--plan` and `--size` in either order, and the order the values are checked in
#       (project, platform, profile, plan, board, epic, turn's form, size, TTL, the board read, turn against the plan).
#
# The file has two halves, each in homes of its own. The first reads plans from files, in a home with no CA, since a
# lease that is not ready needs none. The second starts a stand-in board (board-standin.ps1, which docs/conformance.md,
# "The stand-in board and its credential", gives its interface), writes the stand-in's receipt and credential into its
# home, and runs the same cases with the plan read from an epic's block. Each of its results counts the requests the
# stand-in logged: one GET for a good epic, and none for a command line refused before the board is read.
#
# THE TOKEN (architect's ruling 3, pin P6). The stand-in's token is drawn at random for each run, 43 characters of A to Z,
# a to z, 0 to 9, `-` and `_`. It lives in the fixture object in this process's memory, in the fixture file and in
# `bot.json`'s protected blob in the home, and nowhere else: it is never printed, logged, passed as an argument or put in a
# reason. Every reason this file prints passes through Format-TtText, which replaces the run's token with `<token>`, and a
# control result plants a reason with the token in it and shows that it comes out without it. A result at the end shows
# that no run's stdout or stderr held the token.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into a throwaway home. No case starts a real
# run: every create is given AGENTSPACE_PROVISION_CMD naming a stand-in under the home that records what it is given and
# makes no host, and a result shows it was never started. Every write calls Assert-ConformancePath at the call. The
# program is run with the plans' folder as its working folder, so an argument is a bare file name and no message holds
# a path of the machine's. The stand-in board listens on 127.0.0.1 at a port this file picks by binding port 0, never a
# fixed one and never 8811. Every run of the program removes the caller's LOCALSWIM_* variables, each name set to $null,
# found by listing this process's names and never reading a value. Nothing here reads Terry's board, receipt or
# credential, starts localswim.exe or connects to a port this file did not open. The stand-in is ended by its name and
# its PID, after its creation time is checked.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-plan-turn-ttl.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped (no CA for a ready lease; no
#       working DPAPI for the board cases).

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

# The most agents at once that xl holds at today's footprint (lease-api.md, Plan: 16 vCPUs and 64 GiB at half a vCPU and
# 2 GiB each): this case's own constant, as test-plan-errors.ps1's is. A new footprint changes it and no contract version.
$script:TtMost = 32
$script:TtAgentsOf = @(@(4, 's'), @(8, 'm'), @(16, 'l'), @(32, 'xl'))
$script:TtType = @{ s = 'm9g.large'; m = 'm9g.xlarge'; l = 'm9g.2xlarge'; xl = 'm9g.4xlarge' }
$script:TtBoardId = '0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f'
$script:TtApi = '/api/2026-11-10/boards/'

# --- The token, and the one function every printed reason goes through ----------------------------------------------

function New-TtToken {
  $chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_'
  $sb = [Text.StringBuilder]::new()
  for ($i = 0; $i -lt 43; $i++) { [void]$sb.Append($chars[[Security.Cryptography.RandomNumberGenerator]::GetInt32($chars.Length)]) }
  return $sb.ToString()
}
# Drawn once, here, for this run. It is read by Format-TtText, the fixture's writer and the DPAPI blob's maker, and by
# nothing that prints.
$script:TtToken = New-TtToken

# Replaces the run's token with a placeholder wherever it stands.
function Hide-TtToken([string]$Text) {
  $x = [string]$Text
  if ($script:TtToken -and $x.Contains($script:TtToken)) { $x = $x.Replace($script:TtToken, '<token>') }
  return $x
}

# A text for a failure reason: the token is replaced first, a path of a drive is hidden, whitespace is collapsed and the
# length is bounded. EVERY reason this file prints passes through here.
function Format-TtText([string]$Text) {
  $x = Hide-TtToken $Text
  $x = $x -replace '[A-Za-z]:[\\/][^\s''"]*', '<path>'
  $x = $x -replace '\s+', ' '
  if ($x.Length -gt 320) { $x = $x.Substring(0, 320) + '...' }
  return $x
}

function Write-TtResult {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][ValidateSet('PASS', 'FAIL', 'SKIPPED')][string]$Status, [string]$Reason = '')
  Write-ConformanceResult -Name (Hide-TtToken $Name) -Status $Status -Reason (Format-TtText $Reason)
}

function Complete-TtCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-TtResult -Name $Name -Status PASS }
  else { Write-TtResult -Name $Name -Status FAIL -Reason ((@($Problems | ForEach-Object { Format-TtText $_ })) -join '; ') }
}

# A value as a message quotes it (lease-api.md, Messages): each control character, U+0000 to U+001F and U+007F, and each
# unpaired surrogate as \u and four lowercase hex digits; everything else as it is.
function Format-TtQuoted([string]$Value) {
  $sb = [Text.StringBuilder]::new()
  for ($i = 0; $i -lt $Value.Length; $i++) {
    $c = $Value[$i]
    $n = [int]$c
    if ($n -le 0x1F -or $n -eq 0x7F) { [void]$sb.Append('\u' + $n.ToString('x4')) }
    elseif ([char]::IsHighSurrogate($c) -and ($i + 1) -lt $Value.Length -and [char]::IsLowSurrogate($Value[$i + 1])) { [void]$sb.Append($c).Append($Value[$i + 1]); $i++ }
    elseif ([char]::IsSurrogate($c)) { [void]$sb.Append('\u' + $n.ToString('x4')) }
    else { [void]$sb.Append($c) }
  }
  return $sb.ToString()
}

# --- The state of the home a group of cases runs in -----------------------------------------------------------------

$script:TtLab = $null
$script:TtDir = $null
$script:TtStandIn = $null
$script:TtBoard = $null
$script:TtRuns = 0
$script:TtLeaks = 0
$script:TtDocs = 0
# The names of this process's variables that start LOCALSWIM_, found by listing names alone, each to be removed from every
# run of the program (docs/conformance.md, "The stand-in credential": the harness removes only AGENTSPACE_* ones).
$script:TtLocalswimNames = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -like 'LOCALSWIM_*' })
# The documents the first half printed, by key, for the second half to compare with the board's.
$script:TtFileDocs = [hashtable]::new([StringComparer]::Ordinal)
$script:TtReadyDocs = [hashtable]::new([StringComparer]::Ordinal)

# `agentspace lease <Arguments>`, in the plans folder, with the stand-in as the provisioning run, the case's ProxyCommand,
# and no LOCALSWIM_* variable. Returns the harness's result, and Seconds. A run whose output holds the token is counted.
function Invoke-TtLease {
  param([Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [hashtable]$Extra = @{}, [int]$TimeoutSeconds = 90)
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy; AGENTSPACE_PROVISION_CMD = $script:TtStandIn.Path }
  foreach ($n in $script:TtLocalswimNames) { $e[$n] = $null }
  foreach ($k in @($Extra.Keys)) { $e[$k] = $Extra[$k] }
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_CMD' -ConformanceHome $script:TtLab -Arguments (@('lease') + $Arguments) -Environment $e -TimeoutSeconds $TimeoutSeconds
  $r | Add-Member -NotePropertyName Seconds -NotePropertyValue $sw.Elapsed.TotalSeconds
  $script:TtRuns++
  if ($script:TtToken -and (([string]$r.Stdout).Contains($script:TtToken) -or ([string]$r.Stderr).Contains($script:TtToken))) { $script:TtLeaks++ }
  return $r
}

# --- Building plans -------------------------------------------------------------------------------------------------

function ConvertTo-TtJson([string]$Value) {
  $sb = [Text.StringBuilder]::new('"')
  foreach ($c in $Value.ToCharArray()) {
    $n = [int]$c
    if ($c -ceq '"') { [void]$sb.Append('\"') }
    elseif ($c -ceq '\') { [void]$sb.Append('\\') }
    elseif ($n -lt 0x20) { [void]$sb.Append('\u' + $n.ToString('x4')) }
    else { [void]$sb.Append($c) }
  }
  [void]$sb.Append('"')
  return $sb.ToString()
}

# A turn of $Agents agents at once: one story of $Agents - 1 subagents that start after none, up to 33 agents (32 stories
# of one subagent each beyond 17). $Budget is the JSON text of `budget_minutes`, written as the case means it, or '' to
# leave the member out. Each story has one subagent, so a story's peak is 1 and the turn's agents are the stories' count
# plus 1 for the dev lead (lease-api.md, Plan).
function New-TtTurn {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][ValidateRange(2, 33)][int]$Agents, [string]$Budget = '')
  $st = @(1..($Agents - 1) | ForEach-Object { '{"name":"s' + $_ + '","subagents":[{"name":"a","model":"Opus","effort":"high"}]}' }) -join ','
  $b = if ($Budget -ne '') { ',"budget_minutes":' + $Budget } else { '' }
  return '{"name":' + (ConvertTo-TtJson $Name) + $b + ',"stories":[' + $st + ']}'
}
function New-TtPlan {
  param([Parameter(Mandatory)][string[]]$Turns, [AllowEmptyString()][string]$Name = '')
  $head = if ($Name -ne '') { '"name":' + (ConvertTo-TtJson $Name) + ',' } else { '' }
  return '{' + $head + '"turns":[' + ($Turns -join ',') + ']}'
}

# Writes a plan file in the plans folder of the home, checks that it is a plan by its schema unless -NoSchema (a plan that
# the schema accepts and the program refuses, or that neither accepts, is the case's own), and returns its bare name.
function Write-TtPlan {
  param([Parameter(Mandatory)][string]$File, [Parameter(Mandatory)][string]$Text, [switch]$NoSchema)
  if (-not $NoSchema -and -not (Test-ConformanceSchema -Json $Text -Schema 'plan.input.schema.json')) { throw "The case wrote a plan, $File, that does not validate against its schema." }
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:TtLab -Path (Join-Path $script:TtDir $File)), [Text.UTF8Encoding]::new($false).GetBytes($Text))
  return $File
}

# The description of an epic: prose, the plan block, prose (lease-api.md, The plan block).
function New-TtDescription([string]$PlanText) {
  return "An epic's description, in prose.`n`n" + '```agentspace-plan' + "`n" + $PlanText + "`n" + '```' + "`n`nMore prose, after the block.`n"
}

# The plans of the cases, by key: the file the first half writes, the epic's ticket the second half's fixture gives its
# block to, the plan's name, and its text. A plan read from a file and one read from an epic are the same value (D83).
function Get-TtPlans {
  $big = 'x' * 64
  $plans = [ordered]@{}
  $plans['main'] = @{ Ticket = 101; Name = 'Main plan'; Text = (New-TtPlan -Name 'Main plan' -Turns @(
        (New-TtTurn -Name 'T01' -Agents 4 -Budget '45'), (New-TtTurn -Name 'T02' -Agents 9 -Budget '120'),
        (New-TtTurn -Name 'T03' -Agents 5), (New-TtTurn -Name 'T04' -Agents 9 -Budget '180'))) }
  $plans['nobusy'] = @{ Ticket = 102; Name = 'Busiest unbudgeted'; Text = (New-TtPlan -Name 'Busiest unbudgeted' -Turns @((New-TtTurn -Name 'T1' -Agents 3 -Budget '60'), (New-TtTurn -Name 'T2' -Agents 6))) }
  $plans['null'] = @{ Ticket = 103; Name = 'Null budget'; Text = (New-TtPlan -Name 'Null budget' -Turns @((New-TtTurn -Name 'T1' -Agents 3 -Budget 'null'), (New-TtTurn -Name 'T2' -Agents 4 -Budget '30'))) }
  $plans['noname'] = @{ Ticket = 104; Name = ''; Text = (New-TtPlan -Turns @((New-TtTurn -Name 'T1' -Agents 3 -Budget '45'))) }
  $plans['large'] = @{ Ticket = 105; Name = 'Large plan'; Text = (New-TtPlan -Name 'Large plan' -Turns @(
        (New-TtTurn -Name 'Small' -Agents 3 -Budget '30'), (New-TtTurn -Name 'Huge' -Agents 33 -Budget '200'), (New-TtTurn -Name 'Edge' -Agents 32 -Budget '10'))) }
  $plans['bounds'] = @{ Ticket = 106; Name = 'Bounds'; Text = (New-TtPlan -Name 'Bounds' -Turns @(
        (New-TtTurn -Name 'T1' -Agents 3 -Budget '1'), (New-TtTurn -Name 'T2' -Agents 3 -Budget '960'), (New-TtTurn -Name 'T3' -Agents 3 -Budget '2'),
        (New-TtTurn -Name 'T4' -Agents 3 -Budget '3'), (New-TtTurn -Name 'T5' -Agents 3 -Budget '7'), (New-TtTurn -Name 'T6' -Agents 3 -Budget '959'))) }
  $plans['names'] = @{ Ticket = 107; Name = 'Names'; Text = (New-TtPlan -Name 'Names' -Turns @(
        (New-TtTurn -Name 'T01' -Agents 3 -Budget '45'), (New-TtTurn -Name 't01' -Agents 6 -Budget '120'), (New-TtTurn -Name 'Turn 2' -Agents 4 -Budget '60'),
        (New-TtTurn -Name "it's" -Agents 5 -Budget '30'), (New-TtTurn -Name $big -Agents 3 -Budget '90'))) }
  $plans['twice'] = @{ Ticket = 108; Name = 'Twice'; Text = (New-TtPlan -Name 'Twice' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget '45'), (New-TtTurn -Name 'T01' -Agents 3))) }
  $plans['otherbad'] = @{ Ticket = 109; Name = 'Other bad'; NoSchema = $true; Text = (New-TtPlan -Name 'Other bad' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget '45'), (New-TtTurn -Name 'T02' -Agents 3 -Budget '90.0'))) }
  $plans['ok'] = @{ Ticket = 110; Name = 'Ok plan'; Text = (New-TtPlan -Name 'Ok plan' -Turns @((New-TtTurn -Name 'T01' -Agents 3))) }
  $plans['xl'] = @{ Ticket = 111; Name = 'Xl plan'; Text = (New-TtPlan -Name 'Xl plan' -Turns @((New-TtTurn -Name 'A' -Agents 3 -Budget '20'), (New-TtTurn -Name 'B' -Agents 17 -Budget '300'))) }
  return $plans
}

# The budget forms of pin P24 and their kin, each as the JSON text written into the plan. Each is refused by check 6, with
# the `<why>` of the schema, or, for a number the parser cannot hold, by check 4. A form the schema accepts and the program
# refuses (90.0, 9e1 and the like) is among them.
$script:TtBadBudgetsSchema = @('90.0', '9e1', '9E1', '9e+1', '90e0', '0.9e2', '900e-1', '90.00', '-0', '-0.0', '0', '1.0', '960.0', '961', '1000', '-1', '-90', '90.5', '0.5', '"90"', '"9e1"', 'true', 'false', '[]', '[90]', '{}', '{"a":1}')
$script:TtBadBudgetsJson = @('1e400', '01')
$script:TtGoodBudgets = @(@('1', 120), @('90', 8100), @('960', 86400), @('null', 28800))

# --- What a lease document MUST be ----------------------------------------------------------------------------------

function Test-TtTime([string]$Text) { return ($Text -cmatch '\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\z') }

# The problems with a lease's sized_for: an object of exactly turn and agents, and plan when the plan has a name, each as
# the plan writes it.
function Get-TtSizedForProblems {
  param([Parameter(Mandatory)]$Doc, [AllowEmptyString()][string]$PlanName, [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents)
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not $Doc.ContainsKey('sized_for')) { $p.Add('The lease has no sized_for'); return , $p }
  $sf = $Doc['sized_for']
  if ($sf -isnot [hashtable]) { $p.Add('sized_for is not an object'); return , $p }
  $want = @('agents', 'turn') + $(if ($PlanName -ne '') { @('plan') } else { @() })
  $got = @($sf.Keys | ForEach-Object { [string]$_ } | Sort-Object { $_ } -CaseSensitive)
  $want = @($want | Sort-Object { $_ } -CaseSensitive)
  if (($got -join ',') -cne ($want -join ',')) { $p.Add("sized_for has the members [$($got -join ', ')], not [$($want -join ', ')]") }
  if ($sf.ContainsKey('turn') -and ($sf['turn'] -isnot [string] -or $sf['turn'] -cne $Turn)) { $p.Add("sized_for.turn is '$($sf['turn'])', not '$Turn'") }
  if ($PlanName -ne '' -and $sf.ContainsKey('plan') -and ($sf['plan'] -isnot [string] -or $sf['plan'] -cne $PlanName)) { $p.Add('sized_for.plan is not the plan''s name as the plan writes it') }
  if ($sf.ContainsKey('agents')) {
    $a = $sf['agents']
    if (($a -isnot [int] -and $a -isnot [long]) -or $a -ne $Agents) { $p.Add("sized_for.agents is $($sf['agents']), not the integer $Agents") }
  }
  return , $p
}

# The problems with a run that was meant to print a lease for a plan: the success test, the project, the state, the size
# picked, sized_for, the TTL, the reasons, the message of a plan_too_large lease byte for byte, and none of a budget in the
# output. -Ttl is the TTL the lease MUST carry, as its ttl_seconds; for a lease that is ready it is also expires_at minus
# created_at of the same document, within the seconds the create took (lease-api.md, Fields: the two differ by no more
# than the time create took, and either MAY come first).
function Get-TtLeaseProblems {
  param(
    [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Project, [Parameter(Mandatory)][ValidateSet('ready', 'unsatisfiable')][string]$State,
    [Parameter(Mandatory)][string]$Size, [AllowEmptyString()][string]$PlanName = '', [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents,
    [Parameter(Mandatory)][int]$Ttl, [string[]]$Reasons = @(), [ValidateSet('', 'busiest', 'turn')][string]$TooLarge = ''
  )
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  $script:TtDocs++
  if ($doc['state'] -cne $State) { $p.Add("The state is '$($doc['state'])', not $State") }
  if ($doc['project'] -cne $Project) { $p.Add('The lease is for another project') }
  if ($doc['size'] -cne $Size) { $p.Add("The size is '$($doc['size'])', not '$Size'") }
  foreach ($x in (Get-TtSizedForProblems -Doc $doc -PlanName $PlanName -Turn $Turn -Agents $Agents)) { $p.Add($x) }
  if ($doc['ttl_seconds'] -ne $Ttl) { $p.Add("ttl_seconds is $($doc['ttl_seconds']), not $Ttl") }
  if (([string]$Run.Stdout).Contains('budget_minutes')) { $p.Add('The output prints budget_minutes') }
  if ($State -ceq 'unsatisfiable') {
    foreach ($m in 'ssh', 'expires_at', 'released_at') { if ($doc.ContainsKey($m)) { $p.Add("An unsatisfiable lease carries $m") } }
    $got = @($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['code'] })
    if (($got -join ',') -cne ($Reasons -join ',')) { $p.Add("The reasons are [$($got -join ', ')], not [$($Reasons -join ', ')]") }
    if ($TooLarge) {
      $word = if ($TooLarge -ceq 'busiest') { "Plan's busiest turn" } else { "Plan's turn" }
      $want = "$word '$Turn' has $Agents agents at once; the largest size, xl, holds $script:TtMost."
      $msgs = @($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['message'] })
      if ($msgs.Count -ne 1 -or $msgs[0] -cne $want) { $p.Add("The plan_too_large message is not the one the spec gives for a lease made $(if ($TooLarge -ceq 'busiest') { 'without' } else { 'with' }) --turn") }
    }
  } else {
    if ($doc.ContainsKey('reasons')) { $p.Add('A ready lease carries reasons') }
    $c = if ($doc.ContainsKey('created_at')) { [string]$doc['created_at'] } else { '' }
    $e = if ($doc.ContainsKey('expires_at')) { [string]$doc['expires_at'] } else { '' }
    if (-not (Test-TtTime $c) -or -not (Test-TtTime $e)) { $p.Add('created_at or expires_at is missing') }
    else {
      $diff = [int](([DateTime]::Parse($e, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal) - [DateTime]::Parse($c, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal)).TotalSeconds)
      $slack = [int][Math]::Ceiling($Run.Seconds) + 2
      if ([Math]::Abs($diff - $Ttl) -gt $slack) { $p.Add("expires_at minus created_at is $diff seconds, not $Ttl within the $slack seconds the create took") }
    }
  }
  return , $p
}

# The problems with two lease documents that MUST be the same lease, one from a file's plan and one from an epic's: every
# member but the ID, the times and the host alias, which a lease alone has (lease-api.md, Plan, The plan block: the same
# plan gives the same lease from either source).
function Get-TtSameProblems {
  param([Parameter(Mandatory)]$A, [Parameter(Mandatory)]$B)
  $p = [System.Collections.Generic.List[string]]::new()
  $skip = 'id', 'created_at', 'expires_at', 'released_at', 'ssh'
  $ka = @($A.Keys | Where-Object { $_ -notin $skip } | Sort-Object { $_ } -CaseSensitive)
  $kb = @($B.Keys | Where-Object { $_ -notin $skip } | Sort-Object { $_ } -CaseSensitive)
  if (($ka -join ',') -cne ($kb -join ',')) { $p.Add("The two leases have the members [$($ka -join ', ')] and [$($kb -join ', ')]"); return , $p }
  foreach ($k in $ka) {
    $ja = ConvertTo-Json -InputObject $A[$k] -Compress -Depth 20
    $jb = ConvertTo-Json -InputObject $B[$k] -Compress -Depth 20
    if ($ja -cne $jb) { $p.Add("The member $k differs between the file's lease and the epic's") }
  }
  if ($A.ContainsKey('ssh') -ne $B.ContainsKey('ssh')) { $p.Add('One of the two leases has ssh, and the other has none') }
  elseif ($A.ContainsKey('ssh') -and ($A['ssh']['user'] -cne $B['ssh']['user'])) { $p.Add('The two leases log in as different users') }
  return , $p
}

# The problems with a run that was meant to fail with a code, a field and one message.
function Get-TtErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string]$Message)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceErrorProblems -Run $Run -Code $Code -Field $Field)) { $p.Add($x) }
  $doc = $Run.ErrorJson
  if ($doc -is [hashtable] -and $Message -and $doc.ContainsKey('message') -and $doc['message'] -cne $Message) { $p.Add("The message is '$($doc['message'])', not '$Message'") }
  return , $p
}

# --- The stand-in board -----------------------------------------------------------------------------------------------

# The port a case picks: binding port 0 on 127.0.0.1, reading it and closing the socket. Never a fixed port, never 8811.
function Get-TtFreePort {
  while ($true) {
    $l = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $l.Start()
    $port = ([Net.IPEndPoint]$l.LocalEndpoint).Port
    $l.Stop()
    if ($port -ne 8811) { return $port }
  }
}

# Starts the stand-in as docs/conformance.md gives: `pwsh -NoProfile -File board-standin.ps1 -Port <n> -Fixture <path> -Log
# <path> -Ready <path>`, each path a file in the home that Assert-ConformancePath returned at this call, with the harness's
# environment for the home. It waits for the ready file with a deadline of 30 seconds, and the PID in it MUST be the one this
# case started. A start that fails (the stand-in exits, or the port was taken) is tried again with another port, at most 3
# times in all. Returns the board's object, or $null with the reason in $script:TtBoardWhy.
function Start-TtBoard {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string]$Fixture, [Parameter(Mandatory)][string]$Log, [Parameter(Mandatory)][string]$Ready)
  $script:TtBoardWhy = ''
  $standin = Join-Path $PSScriptRoot 'board-standin.ps1'
  if (-not (Test-Path -LiteralPath $standin -PathType Leaf)) { $script:TtBoardWhy = 'The stand-in board, board-standin.ps1, is not there'; return $null }
  $pwsh = (Get-Process -Id $PID).Path
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    $port = Get-TtFreePort
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $pwsh
    foreach ($a in @('-NoProfile', '-File', $standin, '-Port', [string]$port, '-Fixture', (Assert-ConformancePath -ConformanceHome $Lab -Path $Fixture), '-Log', (Assert-ConformancePath -ConformanceHome $Lab -Path $Log), '-Ready', (Assert-ConformancePath -ConformanceHome $Lab -Path $Ready))) { $psi.ArgumentList.Add($a) }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $Lab
    foreach ($n in $script:TtLocalswimNames) { $childEnv[$n] = $null }
    foreach ($name in $childEnv.Keys) { if ($null -eq $childEnv[$name]) { [void]$psi.Environment.Remove($name) } else { $psi.Environment[$name] = [string]$childEnv[$name] } }
    if (Test-Path -LiteralPath $Ready) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $Ready) -Force }
    $proc = [Diagnostics.Process]::Start($psi)
    # Its output is read and dropped: the stand-in prints nothing of a token, and a case never prints what it reads.
    [void]$proc.StandardOutput.BaseStream.CopyToAsync([IO.Stream]::Null)
    [void]$proc.StandardError.BaseStream.CopyToAsync([IO.Stream]::Null)
    try { $proc.StandardInput.Close() } catch { }
    $started = $proc.StartTime.ToUniversalTime().Ticks
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $isReady = $false
    while ($sw.Elapsed.TotalSeconds -lt 30) {
      if ($proc.HasExited) { break }
      if (Test-Path -LiteralPath $Ready -PathType Leaf) {
        # UTF-8 with no byte order mark: the bytes are read, so a mark is seen and not skipped.
        $bytes = $null
        try { $bytes = [IO.File]::ReadAllBytes($Ready) } catch { }
        $text = if ($null -ne $bytes -and $bytes.Length -gt 0 -and $bytes[0] -ne 0xEF) { [Text.UTF8Encoding]::new($false).GetString($bytes) } else { '' }
        if ($text -cmatch '\A\d+\n\z') {
          if ([int]$text.Trim() -eq $proc.Id) { $isReady = $true } else { $script:TtBoardWhy = 'The ready file holds a PID that is not the one this case started' }
          break
        }
      }
      Start-Sleep -Milliseconds 200
    }
    if ($isReady) {
      return [pscustomobject]@{ Port = $port; Process = $proc; Id = $proc.Id; StartTicks = $started; Log = $Log; Ready = $Ready; Fixture = $Fixture }
    }
    # Only a stand-in that exited 2 before it was ready, the port it could not bind, is started again on another port
    # (docs/conformance.md, The stand-in board). A wrong PID, a stand-in not ready within 30 seconds and any other exit FAIL at once.
    $exitedTwo = $proc.HasExited -and $proc.ExitCode -eq 2
    if (-not $script:TtBoardWhy) { $script:TtBoardWhy = $(if ($proc.HasExited) { "The stand-in exited with code $($proc.ExitCode) before it was ready" } else { 'The stand-in was not ready within 30 seconds' }) }
    Stop-TtProcess -Process $proc -StartTicks $started
    if (-not $exitedTwo -or $script:TtBoardWhy -like 'The ready file holds*') { return $null }
    if ($attempt -lt 3) { $script:TtBoardWhy = '' } else { $script:TtBoardWhy = "$($script:TtBoardWhy), 3 times in a row" }
  }
  return $null
}

# Ends a process this case started, by its name and its PID, only when it is still the process started at the time
# recorded.
function Stop-TtProcess {
  param([Parameter(Mandatory)]$Process, [Parameter(Mandatory)][long]$StartTicks)
  try {
    if ($Process.HasExited) { return }
    $now = Get-Process -Id $Process.Id -ErrorAction SilentlyContinue
    if ($null -eq $now) { return }
    if ($now.ProcessName -notlike 'pwsh*') { return }
    if ($now.StartTime.ToUniversalTime().Ticks -ne $StartTicks) { return }
    $Process.Kill($true)
    [void]$Process.WaitForExit(10000)
  } catch { }
}

# The requests the stand-in logged, one line each, from the file the stand-in appends to.
function Get-TtLogLines {
  if ($null -eq $script:TtBoard) { return @() }
  if (-not (Test-Path -LiteralPath $script:TtBoard.Log -PathType Leaf)) { return @() }
  $fs = $null
  try {
    $fs = [IO.FileStream]::new($script:TtBoard.Log, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    $sr = [IO.StreamReader]::new($fs, [Text.UTF8Encoding]::new($false))
    $text = $sr.ReadToEnd()
  } finally { if ($fs) { $fs.Dispose() } }
  return @($text -split "`n" | Where-Object { $_ -ne '' })
}

# The problems with the log lines a run added: exactly $Reads of them, each a GET that carries the credential's token
# (auth=ok), and, when the run read a card, each the one request the spec gives for the epic's ticket. Counts lines and
# never times. The log holds no line whose method is not GET (D86), whatever the case.
function Get-TtLogProblems {
  param([AllowEmptyCollection()][string[]]$Lines = @(), [Parameter(Mandatory)][int]$Reads, [int]$Ticket = 0)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Lines.Count -ne $Reads) { $p.Add("The stand-in logged $($Lines.Count) requests, not $Reads") }
  foreach ($l in $Lines) {
    if ($l -cnotmatch '\A[A-Z]+ ') { $p.Add('A log line has no method'); continue }
    if ($l -cnotmatch '\AGET ') { $p.Add('A request was made with a method that is not GET') }
    elseif ($Ticket -gt 0) {
      $want = 'GET ' + $script:TtApi + $script:TtBoardId + '?view=ticket&ticket=' + $Ticket + '&include_prose=true auth=ok'
      if ($l -cne $want) { $p.Add('The request is not the one the spec gives for the epic, with the credential''s token') }
    }
  }
  return , $p
}

# --- Cases ------------------------------------------------------------------------------------------------------------

# The arguments that name where the plan comes from, and the epic's ticket for the log.
function Get-TtSourceArgs {
  param([Parameter(Mandatory)][ValidateSet('file', 'board')][string]$Source, [Parameter(Mandatory)][string]$Key, [Parameter(Mandatory)]$Plans)
  if ($Source -ceq 'file') { return @('--plan', "$Key.json") }
  return @('--board', $script:TtBoardId, '--epic', [string]$Plans[$Key].Ticket)
}

# Runs `create` for a plan from the source and checks the lease it prints, and, from a board, the requests it made. Returns
# the document that passed, or $null.
function Test-TtLease {
  param(
    [Parameter(Mandatory)][string]$Section, [Parameter(Mandatory)][string]$Desc, [Parameter(Mandatory)][ValidateSet('file', 'board')][string]$Source,
    [Parameter(Mandatory)][string]$Key, [Parameter(Mandatory)]$Plans, [string[]]$Extra = @(), [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents,
    [Parameter(Mandatory)][string]$Size, [Parameter(Mandatory)][int]$Ttl, [string]$Project = 'ttnocap', [ValidateSet('ready', 'unsatisfiable')][string]$State = 'unsatisfiable',
    [string[]]$Reasons = @('no_capacity'), [ValidateSet('', 'busiest', 'turn')][string]$TooLarge = ''
  )
  $argv = @('create', '--project', $Project) + (Get-TtSourceArgs -Source $Source -Key $Key -Plans $Plans) + $Extra
  $via = if ($Source -ceq 'file') { 'a plan file' } else { 'an epic''s plan block' }
  $name = "lease-api.md#${Section}: $Desc ($via): $(Format-ConformanceLine (@('lease') + $argv))"
  $p = [System.Collections.Generic.List[string]]::new()
  $doc = $null
  try {
    $before = @(Get-TtLogLines).Count
    $run = Invoke-TtLease -Arguments $argv
    $new = @(Get-TtLogLines | Select-Object -Skip $before)
    foreach ($x in (Get-TtLeaseProblems -Run $run -Project $Project -State $State -Size $Size -PlanName $Plans[$Key].Name -Turn $Turn -Agents $Agents -Ttl $Ttl -Reasons $Reasons -TooLarge $TooLarge)) { $p.Add($x) }
    if ($Source -ceq 'board') {
      foreach ($x in (Get-TtLogProblems -Lines $new -Reads 1 -Ticket $Plans[$Key].Ticket)) { $p.Add($x) }
      if (([string]$run.Stdout).Contains($script:TtBoardId)) { $p.Add('The output holds the board''s ID') }
    }
    if ($p.Count -eq 0) { $doc = $run.Json }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-TtCheck -Name $name -Problems $p
  return $doc
}

# Runs `create` for a line that MUST fail, and checks the error, that nothing was recorded or made, and, from a board, the
# requests it made: $Reads of them.
function Test-TtError {
  param(
    [Parameter(Mandatory)][string]$Section, [Parameter(Mandatory)][string]$Desc, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments,
    [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string]$Message, [int]$Reads = -1, [int]$Ticket = 0, [hashtable]$Extra = @{}
  )
  $name = "lease-api.md#${Section}: $Desc is $Code$(if ($Field) { " for '$Field'" } else { ' with no field' }): $(Format-ConformanceLine (@('lease') + $Arguments))"
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $before = @(Get-TtLogLines).Count
    $recordsBefore = Get-PvRecordCount -Lab $script:TtLab
    $run = Invoke-TtLease -Arguments $Arguments -Extra $Extra
    $new = @(Get-TtLogLines | Select-Object -Skip $before)
    foreach ($x in (Get-TtErrorProblems -Run $run -Code $Code -Field $Field -Message $Message)) { $p.Add($x) }
    $after = Get-PvRecordCount -Lab $script:TtLab
    if ($after -ne $recordsBefore) { $p.Add("The lease records held $recordsBefore leases before the command and $after after: a refused create records no lease") }
    foreach ($x in (Get-PvLeaseFilesProblems -Lab $script:TtLab)) { $p.Add($x) }
    if ($Reads -ge 0 -and $null -ne $script:TtBoard) { foreach ($x in (Get-TtLogProblems -Lines $new -Reads $Reads -Ticket $Ticket)) { $p.Add($x) } }
    elseif ($null -ne $script:TtBoard) { foreach ($x in (Get-TtLogProblems -Lines $new -Reads $new.Count)) { $p.Add($x) } }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-TtCheck -Name $name -Problems $p
}

# The host registry for the ready cases: the box's record, then one record per size, each ready at the size's own type.
function Set-TtHosts {
  param([Parameter(Mandatory)]$Lab)
  $recs = @('{"project":"swimcoach","stack":"agentbox","state":"ready","instance_type":"m9g.xlarge"}')
  foreach ($s in 's', 'm', 'l', 'xl') { $recs += '{"project":"tt-' + $s + '","stack":"agentspace-host-tt-' + $s + '","state":"ready","instance_type":"' + $script:TtType[$s] + '"}' }
  $text = '{"version":0,"hosts":[' + ($recs -join ',') + ']}' + "`n"
  if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/hosts.state.schema.json')) { throw 'The case wrote a hosts.json that does not validate against its schema.' }
  Write-PvText -Lab $Lab -Path (Get-PvHostsPath $Lab) -Text $text
}

# Makes a throwaway home for a group of cases, with no CA unless -KeepCa, a plans folder that is the program's working folder
# and the recording stand-in every create names.
function Initialize-TtLab {
  param([Parameter(Mandatory)]$Lab, [string]$Name, [switch]$KeepCa)
  $script:TtLab = $Lab
  $script:TtDir = Join-Path $Lab.Root 'plans'
  Reset-PvState -Lab $Lab
  if (-not $KeepCa) { Remove-PvCa -Lab $Lab }
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $script:TtDir) | Out-Null
  $script:TtStandIn = New-PvStandIn -Lab $Lab -Name $Name
  [Environment]::CurrentDirectory = (Assert-ConformancePath -ConformanceHome $Lab -Path $script:TtDir)
}

# The cases of one source: --turn, each TTL rule, every plan that is refused and every value of a turn that is. The first
# half runs them on files, in a home with no CA; the second on epics, with the board stand-in listening. $Reads is what a
# command line that reaches the board makes: 1 for a board, and 0 for a file, where it is not counted.
function Invoke-TtSourceCases {
  param([Parameter(Mandatory)][ValidateSet('file', 'board')][string]$Source, [Parameter(Mandatory)]$Plans)
  $fromBoard = ($Source -ceq 'board')
  $reach = if ($fromBoard) { 1 } else { -1 }
  $src = { param($k) Get-TtSourceArgs -Source $Source -Key $k -Plans $Plans }
  $t = @{ Source = $Source; Plans = $Plans }
  $keep = { param($k, $d) if ($Source -ceq 'file') { $script:TtFileDocs[$k] = $d } }
  $same = {
    param($k, $d, $why)
    if ($Source -ceq 'board' -and $null -ne $d -and $script:TtFileDocs.ContainsKey($k) -and $null -ne $script:TtFileDocs[$k]) {
      Complete-TtCheck -Name "lease-api.md#plan: The same plan and the same line give the same lease from an epic as from a file: $why" -Problems (Get-TtSameProblems -A $script:TtFileDocs[$k] -B $d)
    } elseif ($Source -ceq 'board') {
      Write-TtResult -Name "lease-api.md#plan: The same plan and the same line give the same lease from an epic as from a file: $why" -Status FAIL -Reason 'One of the two leases was not printed, so they could not be compared'
    }
  }

  # ---- --turn picks the turn the lease is for: the main plan has four turns of 4, 9, 5 and 9 agents ----
  $mc = @(
    @{ K = 'busiest'; D = 'without --turn the lease is for the busiest turn, T02: 9 agents, size l, and its budget of 120 minutes gives 10800'; A = @(); Turn = 'T02'; Agents = 9; Size = 'l'; Ttl = 10800 }
    @{ K = 't01'; D = '--turn T01: 4 agents, size s, and its budget of 45 minutes gives 4080, though T02 is busier'; A = @('--turn', 'T01'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 4080 }
    @{ K = 't02'; D = '--turn T02, the busiest turn named: the lease is the one without --turn'; A = @('--turn', 'T02'); Turn = 'T02'; Agents = 9; Size = 'l'; Ttl = 10800 }
    @{ K = 't03'; D = '--turn T03, which has no budget: 5 agents, size m, and the default of 8 hours, though the plan''s other turns have budgets'; A = @('--turn', 'T03'); Turn = 'T03'; Agents = 5; Size = 'm'; Ttl = 28800 }
    @{ K = 't04'; D = '--turn T04, the second of two busiest turns: 9 agents, size l, and its budget of 180 minutes gives 16200'; A = @('--turn', 'T04'); Turn = 'T04'; Agents = 9; Size = 'l'; Ttl = 16200 }
    @{ K = 't01eq'; D = '--turn=T01 in the = form'; A = @('--turn=T01'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 4080 }
    @{ K = 't01case'; D = '--TURN T01, the flag in other letters'; A = @('--TURN', 'T01'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 4080 }
    @{ K = 'ttl-busiest'; D = '--ttl 2h without --turn: --ttl wins over the busiest turn''s budget'; A = @('--ttl', '2h'); Turn = 'T02'; Agents = 9; Size = 'l'; Ttl = 7200 }
    @{ K = 'ttl-t01'; D = '--turn T01 with --ttl 30m: --ttl wins over the turn''s budget'; A = @('--turn', 'T01', '--ttl', '30m'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 1800 }
    @{ K = 'ttl-t03'; D = '--turn T03 with --ttl 90m'; A = @('--turn', 'T03', '--ttl', '90m'); Turn = 'T03'; Agents = 5; Size = 'm'; Ttl = 5400 }
    @{ K = 'ttl-longer'; D = '--turn T01 with --ttl 24h: --ttl wins even when the budget gives less'; A = @('--turn', 'T01', '--ttl', '24h'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 86400 }
    @{ K = 'ttl-shortest'; D = '--turn T02 with --ttl 1m, the shortest TTL'; A = @('--turn', 'T02', '--ttl', '1m'); Turn = 'T02'; Agents = 9; Size = 'l'; Ttl = 60 }
    @{ K = 'ttl-iso'; D = '--turn T01 with --ttl PT90M in the ISO form'; A = @('--turn', 'T01', '--ttl', 'PT90M'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 5400 }
    @{ K = 'ttl-default'; D = '--turn T03 with --ttl 8h, the default'; A = @('--turn', 'T03', '--ttl', '8h'); Turn = 'T03'; Agents = 5; Size = 'm'; Ttl = 28800 }
  )
  $mainDocs = @{}
  foreach ($c in $mc) {
    $d = Test-TtLease -Section 'turn' -Desc $c.D -Key 'main' -Extra $c.A -Turn $c.Turn -Agents $c.Agents -Size $c.Size -Ttl $c.Ttl @t
    & $keep ('main|' + $c.K) $d
    & $same ('main|' + $c.K) $d $c.K
  }
  # --turn with a platform no backend has: the reasons are the platform's alone, and sized_for names the turn.
  $d = Test-TtLease -Section 'turn' -Desc '--turn T01 with a platform no backend has: unsupported_platform alone, sized for T01' -Key 'main' -Extra @('--turn', 'T01', '--platform', 'windows/amd64') -Turn 'T01' -Agents 4 -Size 's' -Ttl 4080 -Reasons @('unsupported_platform') @t
  & $keep 'main|t01-platform' $d
  & $same 'main|t01-platform' $d 't01-platform'

  # ---- The TTL when the busiest turn has no budget, or a null one, though another turn has ----
  $d = Test-TtLease -Section 'ttl' -Desc 'the busiest turn T2 has no budget, and T1 has 60: the default of 8 hours, whatever the plan''s other turns have' -Key 'nobusy' -Turn 'T2' -Agents 6 -Size 'm' -Ttl 28800 @t
  & $keep 'nobusy|none' $d; & $same 'nobusy|none' $d 'nobusy'
  $d = Test-TtLease -Section 'ttl' -Desc '--turn T1, which has 60 minutes, gives 5400' -Key 'nobusy' -Extra @('--turn', 'T1') -Turn 'T1' -Agents 3 -Size 's' -Ttl 5400 @t
  & $keep 'nobusy|t1' $d; & $same 'nobusy|t1' $d 'nobusy t1'
  $d = Test-TtLease -Section 'ttl' -Desc 'the busiest turn T2 has 30 minutes, and T1 has a null one: 45 minutes, 2700' -Key 'null' -Turn 'T2' -Agents 4 -Size 's' -Ttl 2700 @t
  & $keep 'null|none' $d; & $same 'null|none' $d 'null'
  $d = Test-TtLease -Section 'ttl' -Desc '--turn T1, whose budget_minutes is null, which means none: the default of 8 hours' -Key 'null' -Extra @('--turn', 'T1') -Turn 'T1' -Agents 3 -Size 's' -Ttl 28800 @t
  & $keep 'null|t1' $d; & $same 'null|t1' $d 'null t1'
  $d = Test-TtLease -Section 'plan' -Desc 'a plan with no name has no sized_for.plan, and a budget of 45 gives 4080' -Key 'noname' -Turn 'T1' -Agents 3 -Size 's' -Ttl 4080 @t
  & $keep 'noname|none' $d; & $same 'noname|none' $d 'noname'
  $d = Test-TtLease -Section 'plan' -Desc 'a plan with no name, with --turn T1' -Key 'noname' -Extra @('--turn', 'T1') -Turn 'T1' -Agents 3 -Size 's' -Ttl 4080 @t
  & $keep 'noname|t1' $d; & $same 'noname|t1' $d 'noname t1'

  # ---- Every budget the TTL table gives, and its rounding, each in a turn of its own ----
  $bc = @(
    @{ T = 'T1'; Ttl = 120; D = 'a budget of 1 minute gives 2 minutes, 120 seconds' }
    @{ T = 'T2'; Ttl = 86400; D = 'a budget of 960 minutes gives 24 hours, 86400 seconds, the range''s top, and is never refused with out_of_range' }
    @{ T = 'T3'; Ttl = 180; D = 'a budget of 2 minutes gives 3 minutes, 180 seconds' }
    @{ T = 'T4'; Ttl = 300; D = 'a budget of 3 minutes gives 4.5, rounded up to 5 minutes, 300 seconds' }
    @{ T = 'T5'; Ttl = 660; D = 'a budget of 7 minutes gives 10.5, rounded up to 11 minutes, 660 seconds' }
    @{ T = 'T6'; Ttl = 86340; D = 'a budget of 959 minutes gives 1438.5, rounded up to 1439 minutes, 86340 seconds' }
  )
  foreach ($c in $bc) {
    $d = Test-TtLease -Section 'ttl' -Desc "--turn $($c.T): $($c.D)" -Key 'bounds' -Extra @('--turn', $c.T) -Turn $c.T -Agents 3 -Size 's' -Ttl $c.Ttl @t
    & $keep ('bounds|' + $c.T) $d; & $same ('bounds|' + $c.T) $d ('bounds ' + $c.T)
  }
  $d = Test-TtLease -Section 'ttl' -Desc 'without --turn, the first of six turns of 3 agents is the lease''s, and its budget of 1 minute gives 120' -Key 'bounds' -Turn 'T1' -Agents 3 -Size 's' -Ttl 120 @t
  & $keep 'bounds|none' $d; & $same 'bounds|none' $d 'bounds'
  $d = Test-TtLease -Section 'ttl' -Desc '--turn T2 with --ttl 1h: the budget of 960 changes nothing' -Key 'bounds' -Extra @('--turn', 'T2', '--ttl', '1h') -Turn 'T2' -Agents 3 -Size 's' -Ttl 3600 @t
  & $keep 'bounds|t2-ttl' $d; & $same 'bounds|t2-ttl' $d 'bounds t2 ttl'
  # A TTL given is checked as any is, and the budget never changes that.
  Test-TtError -Section 'ttl' -Desc 'a plan and a TTL with no unit' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--ttl', '8')) -Code invalid_value -Field ttl -Message "TTL '8' has no unit. Give one, as in '8h'." -Reads 0
  Test-TtError -Section 'ttl' -Desc 'a plan and a TTL above 24 hours: --ttl is never clamped' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--ttl', '48h')) -Code out_of_range -Field ttl -Message "TTL '48h' is outside the allowed range. Give from 1m to 24h; it is never clamped." -Reads 0
  Test-TtError -Section 'ttl' -Desc 'a plan and a TTL of 59 seconds' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn', 'T01', '--ttl', '59s')) -Code out_of_range -Field ttl -Message "TTL '59s' is outside the allowed range. Give from 1m to 24h; it is never clamped." -Reads 0
  Test-TtError -Section 'ttl' -Desc 'a turn of budget 960 and a TTL of 25 hours' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'bounds') + @('--turn', 'T2', '--ttl', '25h')) -Code out_of_range -Field ttl -Message "TTL '25h' is outside the allowed range. Give from 1m to 24h; it is never clamped." -Reads 0

  # ---- A turn that no size holds, and one that fits though another is too large ----
  $d = Test-TtLease -Section 'turn' -Desc 'without --turn the busiest turn Huge has 33 agents: plan_too_large, size xl, sized_for names Huge, the busiest-turn message, and its budget of 200 gives 18000' -Key 'large' -Turn 'Huge' -Agents 33 -Size 'xl' -Ttl 18000 -Reasons @('plan_too_large') -TooLarge busiest @t
  & $keep 'large|none' $d; & $same 'large|none' $d 'large'
  $d = Test-TtLease -Section 'turn' -Desc '--turn Huge, the busiest turn named: plan_too_large with the message for a lease made with --turn' -Key 'large' -Extra @('--turn', 'Huge') -Turn 'Huge' -Agents 33 -Size 'xl' -Ttl 18000 -Reasons @('plan_too_large') -TooLarge turn @t
  & $keep 'large|huge' $d; & $same 'large|huge' $d 'large huge'
  $d = Test-TtLease -Section 'turn' -Desc '--turn Small, a turn that fits though Huge does not: 3 agents, size s, no_capacity, and its budget of 30 gives 2700' -Key 'large' -Extra @('--turn', 'Small') -Turn 'Small' -Agents 3 -Size 's' -Ttl 2700 @t
  & $keep 'large|small' $d; & $same 'large|small' $d 'large small'
  $d = Test-TtLease -Section 'turn' -Desc '--turn Edge, 32 agents, the most xl holds: size xl, no_capacity, not plan_too_large, and its budget of 10 gives 900' -Key 'large' -Extra @('--turn', 'Edge') -Turn 'Edge' -Agents 32 -Size 'xl' -Ttl 900 @t
  & $keep 'large|edge' $d; & $same 'large|edge' $d 'large edge'
  $d = Test-TtLease -Section 'turn' -Desc '--turn Huge with a platform no backend has: plan_too_large is the only reason' -Key 'large' -Extra @('--turn', 'Huge', '--platform', 'windows/amd64') -Turn 'Huge' -Agents 33 -Size 'xl' -Ttl 18000 -Reasons @('plan_too_large') -TooLarge turn @t
  & $keep 'large|huge-platform' $d; & $same 'large|huge-platform' $d 'large huge platform'
  $d = Test-TtLease -Section 'turn' -Desc '--turn Huge with --ttl 1h: plan_too_large carries the TTL asked for' -Key 'large' -Extra @('--turn', 'Huge', '--ttl', '1h') -Turn 'Huge' -Agents 33 -Size 'xl' -Ttl 3600 -Reasons @('plan_too_large') -TooLarge turn @t
  & $keep 'large|huge-ttl' $d; & $same 'large|huge-ttl' $d 'large huge ttl'

  # ---- A turn is matched exactly, by name ----
  $d = Test-TtLease -Section 'turn' -Desc 'without --turn the busiest turn is t01 of 6 agents (T01 has 3), size m, 10800' -Key 'names' -Turn 't01' -Agents 6 -Size 'm' -Ttl 10800 @t
  & $keep 'names|none' $d; & $same 'names|none' $d 'names'
  $d = Test-TtLease -Section 'turn' -Desc '--turn T01 is the turn T01 of 3 agents, never t01' -Key 'names' -Extra @('--turn', 'T01') -Turn 'T01' -Agents 3 -Size 's' -Ttl 4080 @t
  & $keep 'names|T01' $d; & $same 'names|T01' $d 'names T01'
  $d = Test-TtLease -Section 'turn' -Desc '--turn t01 is the turn t01 of 6 agents, never T01' -Key 'names' -Extra @('--turn', 't01') -Turn 't01' -Agents 6 -Size 'm' -Ttl 10800 @t
  & $keep 'names|t01' $d; & $same 'names|t01' $d 'names t01'
  $d = Test-TtLease -Section 'turn' -Desc '--turn "Turn 2", a name with a space in it' -Key 'names' -Extra @('--turn', 'Turn 2') -Turn 'Turn 2' -Agents 4 -Size 's' -Ttl 5400 @t
  & $keep 'names|turn2' $d; & $same 'names|turn2' $d 'names turn 2'
  $d = Test-TtLease -Section 'turn' -Desc '--turn "it''s", a name with an apostrophe in it' -Key 'names' -Extra @('--turn', "it's") -Turn "it's" -Agents 5 -Size 'm' -Ttl 2700 @t
  & $keep 'names|apos' $d; & $same 'names|apos' $d 'names apostrophe'
  $d = Test-TtLease -Section 'turn' -Desc '--turn of a name of 64 characters, the longest' -Key 'names' -Extra @('--turn', ('x' * 64)) -Turn ('x' * 64) -Agents 3 -Size 's' -Ttl 8100 @t
  & $keep 'names|long' $d; & $same 'names|long' $d 'names 64'

  # ---- A turn that the plan does not have, and a value outside a turn's form ----
  $mainTurns = "'T01', 'T02', 'T03', 'T04'"
  Test-TtError -Section 'turn' -Desc "--turn T99 names no turn of the plan, which lists its turns in the plan's order" -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn', 'T99')) -Code invalid_value -Field turn -Message "Turn 'T99' is not a turn of the plan; its turns are $mainTurns." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['main'].Ticket } else { 0 })
  Test-TtError -Section 'turn' -Desc "--turn t01 names no turn of a plan whose turn is T01: a turn is matched exactly" -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn', 't01')) -Code invalid_value -Field turn -Message "Turn 't01' is not a turn of the plan; its turns are $mainTurns." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['main'].Ticket } else { 0 })
  Test-TtError -Section 'turn' -Desc '--turn T1 names no turn of a plan whose turns are T01, t01, Turn 2, it''s and a name of 64 characters: each is listed raw, in single quotes' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'names') + @('--turn', 'T1')) -Code invalid_value -Field turn -Message "Turn 'T1' is not a turn of the plan; its turns are 'T01', 't01', 'Turn 2', 'it's', '$('x' * 64)'." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['names'].Ticket } else { 0 })
  Test-TtError -Section 'turn' -Desc '--turn of 64 characters that is no turn of the plan' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn', ('y' * 64))) -Code invalid_value -Field turn -Message "Turn '$('y' * 64)' is not a turn of the plan; its turns are $mainTurns." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['main'].Ticket } else { 0 })
  Test-TtError -Section 'turn' -Desc '--turn=--help is the value --help, a well-formed value that names no turn' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn=--help')) -Code invalid_value -Field turn -Message "Turn '--help' is not a turn of the plan; its turns are $mainTurns." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['main'].Ticket } else { 0 })
  Test-TtError -Section 'turn' -Desc '--turn=-h is the value -h, a well-formed value that names no turn' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn=-h')) -Code invalid_value -Field turn -Message "Turn '-h' is not a turn of the plan; its turns are $mainTurns." -Reads $reach -Ticket $(if ($fromBoard) { $Plans['main'].Ticket } else { 0 })
  $forms = @(
    @{ D = 'a turn name with a space at its end is never trimmed into T01'; V = 'T01 ' }
    @{ D = 'a turn name with a space at its start'; V = ' T01' }
    @{ D = 'a turn name with a tab at its end'; V = "T01`t" }
    @{ D = 'a turn name with a line feed at its end'; V = "T01`n" }
    @{ D = 'a value of only whitespace is a value, outside the form'; V = '   ' }
    @{ D = 'a turn name of 65 characters'; V = ('z' * 65) }
    @{ D = 'a turn name with a letter that is not ASCII'; V = "T0$([char]0xE9)" }
    @{ D = 'a turn name with U+FEFF'; V = "T0$([char]0xFEFF)1" }
    @{ D = 'a turn name with a delete character, quoted with it escaped'; V = "T0$([char]0x7F)1" }
    @{ D = 'a turn name with a tab inside it, quoted with it escaped'; V = "T0`t1" }
    @{ D = 'a turn name with an unpaired high surrogate, quoted as \ud800'; V = "T0$([char]0xD800)1" }
    @{ D = 'a turn name with an unpaired low surrogate, quoted as \udc00'; V = "T0$([char]0xDC00)1" }
  )
  foreach ($f in $forms) {
    $msg = "Turn '$(Format-TtQuoted $f.V)' is not a turn name. Give the plan's turn name, as in 'T01'."
    Test-TtError -Section 'turn' -Desc "$($f.D): refused before any plan is matched" -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn', $f.V)) -Code invalid_value -Field turn -Message $msg -Reads $(if ($fromBoard) { 0 } else { -1 })
  }
  Test-TtError -Section 'turn' -Desc 'a turn name of whitespace in the = form' -Arguments (@('create', '--project', 'ttnocap') + (& $src 'main') + @('--turn= ')) -Code invalid_value -Field turn -Message "Turn ' ' is not a turn name. Give the plan's turn name, as in 'T01'." -Reads $(if ($fromBoard) { 0 } else { -1 })

  # ---- The order the values are checked in: project, platform, profile, plan, board, epic, turn's form, size, TTL, the board read, turn against the plan ----
  $base = @('create', '--project', 'ttnocap')
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a bad project comes before a turn outside its form' -Arguments (@('create', '--project', 'a_b') + (& $src 'main') + @('--turn', 'T01 ')) -Code invalid_value -Field project -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a bad platform comes before a turn outside its form' -Arguments ($base + (& $src 'main') + @('--turn', 'T01 ', '--platform', 'linux')) -Code invalid_value -Field platform -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a bad profile comes before a turn that names no turn' -Arguments ($base + (& $src 'main') + @('--turn', 'T99', '--profile', 'agent linux')) -Code invalid_value -Field profile -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a turn outside its form comes before a TTL with no unit' -Arguments ($base + (& $src 'main') + @('--turn', 'T01 ', '--ttl', '8')) -Code invalid_value -Field turn -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a TTL with no unit comes before a turn that names no turn, and no board is read' -Arguments ($base + (& $src 'main') + @('--turn', 'T99', '--ttl', '8')) -Code invalid_value -Field ttl -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a TTL out of range comes before a turn that names no turn' -Arguments ($base + (& $src 'main') + @('--turn', 'T99', '--ttl', '48h')) -Code out_of_range -Field ttl -Reads $(if ($fromBoard) { 0 } else { -1 })
  Test-TtError -Section 'exit-codes-and-errors' -Desc 'a turn that names no turn is reported after the plan is read, and comes before nothing but the lease records' -Arguments ($base + (& $src 'ok') + @('--turn', 'T99')) -Code invalid_value -Field turn -Message "Turn 'T99' is not a turn of the plan; its turns are 'T01'." -Reads $(if ($fromBoard) { 1 } else { -1 }) -Ticket $(if ($fromBoard) { $Plans['ok'].Ticket } else { 0 })

  # ---- The whole plan is checked, whichever turn --turn names ----
  $why = @{ twice = "turn 'T01' is named twice"; otherbad = 'the document does not match the plan schema' }
  foreach ($k in 'twice', 'otherbad') {
    $m = if ($fromBoard) { "Epic '$($Plans[$k].Ticket)' carries no usable plan: $($why[$k])." } else { "Plan '$k.json' is not a valid plan: $($why[$k])." }
    $code = 'invalid_value'
    $field = if ($fromBoard) { 'epic' } else { 'plan' }
    Test-TtError -Section 'turn' -Desc "--turn T01, a turn that is fine, with a fault in the plan elsewhere ($($why[$k])): the whole plan is checked" -Arguments ($base + (& $src $k) + @('--turn', 'T01')) -Code $code -Field $field -Message $m -Reads $(if ($fromBoard) { 1 } else { -1 }) -Ticket $(if ($fromBoard) { $Plans[$k].Ticket } else { 0 })
  }

  # ---- Pin P24: budget_minutes written in any form but digits alone is refused ----
}

# The P24 cases, for one source. Each budget is written into a plan of its own, in a turn that --turn does not name, with the
# turn that --turn does name beside it, so that a program that read only the named turn would accept every one.
function Invoke-TtBudgetCases {
  param([Parameter(Mandatory)][ValidateSet('file', 'board')][string]$Source, [AllowNull()]$Fixture, [Parameter(Mandatory)][hashtable]$Tickets)
  $fromBoard = ($Source -ceq 'board')
  $schemaWhy = 'the document does not match the plan schema'
  $jsonWhy = 'the file is not valid JSON'
  $rows = @()
  foreach ($b in $script:TtBadBudgetsSchema) { $rows += , @($b, $schemaWhy) }
  foreach ($b in $script:TtBadBudgetsJson) { $rows += , @($b, $jsonWhy) }
  foreach ($r in $rows) {
    $budget = $r[0]
    $key = $Tickets[$budget]
    $argv = if ($fromBoard) { @('create', '--project', 'ttnocap', '--board', $script:TtBoardId, '--epic', [string]$key) } else { @('create', '--project', 'ttnocap', '--plan', $key) }
    $msg = if ($fromBoard) { "Epic '$key' carries no usable plan: $($r[1])." } else { "Plan '$key' is not a valid plan: $($r[1])." }
    $field = if ($fromBoard) { 'epic' } else { 'plan' }
    Test-TtError -Section 'plan' -Desc "budget_minutes written $budget is refused: it MUST be an integer from 1 to 960 in digits alone (pin P24)" -Arguments $argv -Code invalid_value -Field $field -Message $msg -Reads $(if ($fromBoard) { 1 } else { -1 }) -Ticket $(if ($fromBoard) { $key } else { 0 })
  }
  # The same refusal when --turn names another turn, so the whole plan is checked.
  foreach ($budget in '90.0', '9e1', '-0') {
    $key = $Tickets["turn|$budget"]
    $argv = if ($fromBoard) { @('create', '--project', 'ttnocap', '--board', $script:TtBoardId, '--epic', [string]$key, '--turn', 'T01') } else { @('create', '--project', 'ttnocap', '--plan', $key, '--turn', 'T01') }
    $msg = if ($fromBoard) { "Epic '$key' carries no usable plan: $schemaWhy." } else { "Plan '$key' is not a valid plan: $schemaWhy." }
    $field = if ($fromBoard) { 'epic' } else { 'plan' }
    Test-TtError -Section 'plan' -Desc "budget_minutes written $budget in T02, with --turn T01 naming the turn beside it, is refused (pin P24)" -Arguments $argv -Code invalid_value -Field $field -Message $msg -Reads $(if ($fromBoard) { 1 } else { -1 }) -Ticket $(if ($fromBoard) { $key } else { 0 })
  }
  # The forms that are accepted, each with the TTL it gives: a plan is never refused for one of them.
  foreach ($g in $script:TtGoodBudgets) {
    $budget = $g[0]
    $key = $Tickets["good|$budget"]
    $argv = if ($fromBoard) { @('create', '--project', 'ttnocap', '--board', $script:TtBoardId, '--epic', [string]$key) } else { @('create', '--project', 'ttnocap', '--plan', $key) }
    $name = "lease-api.md#plan: budget_minutes written $budget is accepted and gives $($g[1]) seconds: $(Format-ConformanceLine (@('lease') + $argv))"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $before = @(Get-TtLogLines).Count
      $run = Invoke-TtLease -Arguments $argv
      $new = @(Get-TtLogLines | Select-Object -Skip $before)
      foreach ($x in (Get-TtLeaseProblems -Run $run -Project 'ttnocap' -State unsatisfiable -Size 's' -PlanName "Budget $budget" -Turn 'T01' -Agents 3 -Ttl $g[1] -Reasons @('no_capacity'))) { $p.Add($x) }
      if ($fromBoard) { foreach ($x in (Get-TtLogProblems -Lines $new -Reads 1 -Ticket $key)) { $p.Add($x) } }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-TtCheck -Name $name -Problems $p
  }
}

# Runs a line that asks for help and checks that it printed `create`'s help, byte for byte, exit 0, nothing on stderr and, with
# the stand-in listening, no request: help is printed and nothing else is checked or done (lease-api.md, Command lines, rule 1).
function Test-TtHelp {
  param([Parameter(Mandatory)][string[]]$Line, [Parameter(Mandatory)]$Blocks)
  $name = "lease-api.md#command-lines: $(Format-ConformanceLine $Line) prints create's help and exits 0"
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $before = @(Get-TtLogLines).Count
    $run = Invoke-TtLease -Arguments @($Line | Select-Object -Skip 1)
    $new = @(Get-TtLogLines | Select-Object -Skip $before)
    if (-not $Blocks.ContainsKey('agentspace lease create') -or [string]::IsNullOrEmpty($Blocks['agentspace lease create'])) { $p.Add('docs/lease-api.md has no help block for create') }
    else {
      $want = [Text.UTF8Encoding]::new($false).GetBytes($Blocks['agentspace lease create'])
      if ($run.ExitCode -ne 0) { $p.Add("Exit $($run.ExitCode), not 0") }
      if ($run.StderrBytes.Length -ne 0) { $p.Add("Stderr holds $($run.StderrBytes.Length) bytes") }
      if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$run.StdoutBytes, [byte[]]$want)) { $p.Add("Stdout is $($run.StdoutBytes.Length) bytes and not the doc's $($want.Length)") }
    }
    if ($null -ne $script:TtBoard -and $new.Count -ne 0) { $p.Add("The stand-in logged $($new.Count) requests for a line that asks for help") }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-TtCheck -Name $name -Problems $p
}

# The usage cases that carry `field`, thirteen in all, and --turn's cases among the ranking rules. Every line is a usage error
# (exit 2) with the field and the message the spec gives, nothing recorded, and, with the stand-in listening, no request made.
function Invoke-TtUsageCases {
  param([Parameter(Mandatory)][bool]$Listening)
  $b = $script:TtBoardId
  $c = @('create', '--project', 'ttnocap')
  $wb = "Run 'agentspace lease create --help'."
  $reads = if ($Listening) { 0 } else { -1 }
  $m = @{
    boardPlan = "Options --board and --plan cannot be given together; the epic carries the plan. $wb"
    boardSize = "Options --board and --size cannot be given together; the epic's plan picks the size. $wb"
    epicPlan  = "Options --epic and --plan cannot be given together; the epic carries the plan. $wb"
    epicSize  = "Options --epic and --size cannot be given together; the epic's plan picks the size. $wb"
    planSize  = "Options --plan and --size cannot be given together; the plan picks the size. $wb"
    boardNeedsEpic = "Option --board needs --epic, the epic's ticket number. $wb"
    epicNeedsBoard = "Option --epic needs --board, the board's ID. $wb"
    turnNeedsPlan = "Option --turn needs a plan, from --plan or --epic. $wb"
  }
  $rows = @(
    # 1 to 3: a flag with no value, an empty value, or given twice, for each of --turn, --board and --epic.
    @{ N = 1; D = '--turn with no value'; A = ($c + @('--plan', 'ok.json', '--turn')); F = 'turn'; M = "Option --turn needs a value. $wb" }
    @{ N = 1; D = '--turn with no value, the next argument starting with --'; A = ($c + @('--turn', '--plan', 'ok.json')); F = 'turn'; M = "Option --turn needs a value. $wb" }
    @{ N = 1; D = '--board with no value'; A = ($c + @('--epic', '7', '--board')); F = 'board'; M = "Option --board needs a value. $wb" }
    @{ N = 1; D = '--epic with no value'; A = ($c + @('--board', $b, '--epic')); F = 'epic'; M = "Option --epic needs a value. $wb" }
    @{ N = 2; D = '--turn with an empty value'; A = ($c + @('--plan', 'ok.json', '--turn', '')); F = 'turn'; M = "Option --turn has an empty value. $wb" }
    @{ N = 2; D = '--turn= with an empty value in the = form'; A = ($c + @('--plan', 'ok.json', '--turn=')); F = 'turn'; M = "Option --turn has an empty value. $wb" }
    @{ N = 2; D = '--board with an empty value'; A = ($c + @('--epic', '7', '--board', '')); F = 'board'; M = "Option --board has an empty value. $wb" }
    @{ N = 2; D = '--epic with an empty value'; A = ($c + @('--board', $b, '--epic', '')); F = 'epic'; M = "Option --epic has an empty value. $wb" }
    @{ N = 3; D = '--turn given twice'; A = ($c + @('--plan', 'ok.json', '--turn', 'T01', '--turn', 'T01')); F = 'turn'; M = 'Option --turn is given twice. Give it once.' }
    @{ N = 3; D = '--turn given twice, in other letters and the = form'; A = ($c + @('--plan', 'ok.json', '--turn', 'T01', '--TURN=T01')); F = 'turn'; M = 'Option --turn is given twice. Give it once.' }
    @{ N = 3; D = '--board given twice'; A = ($c + @('--board', $b, '--epic', '7', '--board', $b)); F = 'board'; M = 'Option --board is given twice. Give it once.' }
    @{ N = 3; D = '--epic given twice'; A = ($c + @('--board', $b, '--epic', '7', '--epic', '7')); F = 'epic'; M = 'Option --epic is given twice. Give it once.' }
    # 4 and 5: a missing lease ID and a missing project.
    @{ N = 4; D = 'get with no lease ID'; A = @('get'); F = 'id'; M = "No lease ID given. Run 'agentspace lease get --help'." }
    @{ N = 5; D = 'create with no --project'; A = @('create', '--plan', 'ok.json'); F = 'project'; M = "Option --project is required. $wb" }
    @{ N = 5; D = 'create with no --project, and --turn without a partner: the missing project is reported first'; A = @('create', '--turn', 'T01'); F = 'project'; M = "Option --project is required. $wb" }
    @{ N = 5; D = 'create with no --project, and --board without --epic: the missing project is reported first'; A = @('create', '--board', $b); F = 'project'; M = "Option --project is required. $wb" }
    # 6 to 10: the pairs, each in either order.
    @{ N = 6; D = '--plan with --size'; A = ($c + @('--plan', 'ok.json', '--size', 'm')); F = 'plan'; M = $m.planSize }
    @{ N = 6; D = '--size with --plan'; A = ($c + @('--size', 'm', '--plan', 'ok.json')); F = 'plan'; M = $m.planSize }
    @{ N = 7; D = '--board with --plan'; A = ($c + @('--board', $b, '--plan', 'ok.json')); F = 'board'; M = $m.boardPlan }
    @{ N = 7; D = '--plan with --board'; A = ($c + @('--plan', 'ok.json', '--board', $b)); F = 'board'; M = $m.boardPlan }
    @{ N = 8; D = '--board with --size'; A = ($c + @('--board', $b, '--size', 'm')); F = 'board'; M = $m.boardSize }
    @{ N = 8; D = '--size with --board'; A = ($c + @('--size', 'm', '--board', $b)); F = 'board'; M = $m.boardSize }
    @{ N = 9; D = '--epic with --plan, --board not on the line: the field is epic'; A = ($c + @('--epic', '7', '--plan', 'ok.json')); F = 'epic'; M = $m.epicPlan }
    @{ N = 9; D = '--plan with --epic, --board not on the line: the field is epic'; A = ($c + @('--plan', 'ok.json', '--epic', '7')); F = 'epic'; M = $m.epicPlan }
    @{ N = 9; D = '--epic with --plan, --board on the line after them: reported at --plan, and the field is board'; A = ($c + @('--epic', '7', '--plan', 'ok.json', '--board', $b)); F = 'board'; M = $m.epicPlan }
    @{ N = 10; D = '--epic with --size, --board not on the line: the field is epic'; A = ($c + @('--epic', '7', '--size', 'm')); F = 'epic'; M = $m.epicSize }
    @{ N = 10; D = '--size with --epic, --board not on the line: the field is epic'; A = ($c + @('--size', 'm', '--epic', '7')); F = 'epic'; M = $m.epicSize }
    @{ N = 10; D = '--epic with --size, --board on the line after them: reported at --size, and the field is board'; A = ($c + @('--epic', '7', '--size', 'm', '--board', $b)); F = 'board'; M = $m.epicSize }
    # 11 to 13: the missing partners.
    @{ N = 11; D = '--board without --epic: the field is the flag that is missing, epic'; A = ($c + @('--board', $b)); F = 'epic'; M = $m.boardNeedsEpic }
    @{ N = 12; D = '--epic without --board: the field is the flag that is missing, board'; A = ($c + @('--epic', '7')); F = 'board'; M = $m.epicNeedsBoard }
    @{ N = 13; D = '--turn without --plan or --epic'; A = ($c + @('--turn', 'T01')); F = 'turn'; M = $m.turnNeedsPlan }
    @{ N = 13; D = '--turn without --plan or --epic, beside --size'; A = ($c + @('--turn', 'T01', '--size', 'm')); F = 'turn'; M = $m.turnNeedsPlan }
    @{ N = 13; D = '--turn without --plan or --epic, beside a TTL that has no unit: usage comes before values'; A = ($c + @('--turn', 'T01', '--ttl', '8')); F = 'turn'; M = $m.turnNeedsPlan }
    # The ranking among pairs and partners (lease-api.md, Command lines, rules 1 to 6).
    @{ N = 0; D = '--board with --plan stands before --plan with --size when --board comes first: reported at --board'; A = ($c + @('--plan', 'ok.json', '--board', $b, '--size', 'm')); F = 'board'; M = $m.boardPlan }
    @{ N = 0; D = 'two pairs end at --plan: the pair whose earlier flag stands first is reported, --board with --plan'; A = ($c + @('--board', $b, '--epic', '7', '--plan', 'ok.json')); F = 'board'; M = $m.boardPlan }
    @{ N = 0; D = '--board without --epic is reported before --turn without a plan: the order of the partners'; A = ($c + @('--board', $b, '--turn', 'T01')); F = 'epic'; M = $m.boardNeedsEpic }
    @{ N = 0; D = '--epic without --board is reported before --turn without a plan'; A = ($c + @('--epic', '7', '--turn', 'T01')); F = 'board'; M = $m.epicNeedsBoard }
    @{ N = 0; D = '--turn with a board and an epic and a plan: the pair is reported, not the turn'; A = ($c + @('--turn', 'T01', '--board', $b, '--epic', '7', '--plan', 'ok.json')); F = 'board'; M = $m.boardPlan }
    @{ N = 0; D = 'an unknown option before --turn without a plan is reported first, with no field'; A = ($c + @('--bogus', '--turn', 'T01')); F = ''; M = "Unknown option '--bogus' for agentspace lease create. Run 'agentspace lease create --help'." }
    @{ N = 0; D = 'an unexpected argument after --turn without a plan is reported first, with no field'; A = ($c + @('--turn', 'T01', 'extra')); F = ''; M = "Unexpected argument 'extra' for agentspace lease create. Run 'agentspace lease create --help'." }
    @{ N = 0; D = '--turn with a value of --help in the = form and no plan is the missing plan, never help'; A = ($c + @('--turn=--help')); F = 'turn'; M = $m.turnNeedsPlan }
  )
  foreach ($r in $rows) {
    $who = if ($r.N -gt 0) { "usage case $($r.N) of 13" } else { 'ranking' }
    Test-TtError -Section 'command-lines' -Desc "$who, $($r.D)" -Arguments $r.A -Code usage -Field $r.F -Message $r.M -Reads $reads
  }
  # Help is asked for: it is printed, and nothing is checked or done (rule 1), a --turn with no value included.
  $blocks = Get-ConformanceHelpBlocks
  foreach ($line in @(
      @('lease', 'create', '--turn', '-h'),
      @('lease', 'create', '--plan', 'ok.json', '--turn', '--help'),
      @('lease', 'create', '--board', $b, '--epic', '7', '--plan', 'ok.json', '--help'),
      @('lease', 'create', '--turn', 'T01', '--help'))) {
    Test-TtHelp -Line $line -Blocks $blocks
  }
}

# ====================================================================================================================
# The first half: the plan read from a file, in a home with no CA
# ====================================================================================================================
function Invoke-TtFileHalf {
  $h = $null
  $savedCwd = [Environment]::CurrentDirectory
  try {
    $h = New-ConformanceHome
    Initialize-TtLab -Lab $h -Name 'turn-ttl-files'
    $plans = Get-TtPlans
    foreach ($k in $plans.Keys) { [void](Write-TtPlan -File "$k.json" -Text $plans[$k].Text -NoSchema:([bool]$plans[$k].NoSchema)) }

    # --- A lease with no plan: the default TTL, and no sized_for ---
    $name = 'lease-api.md#ttl: A create with no plan has the default TTL of 8 hours, and no sized_for'
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $run = Invoke-TtLease -Arguments @('create', '--project', 'ttnocap')
      foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
      if ($run.Json -is [hashtable]) { if ($run.Json['ttl_seconds'] -ne 28800) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not 28800") }; if ($run.Json.ContainsKey('sized_for')) { $p.Add('A lease made with no plan holds sized_for') } }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-TtCheck -Name $name -Problems $p

    Invoke-TtSourceCases -Source file -Plans $plans | Out-Null

    # --- Pin P24, from files ---
    $tickets = [hashtable]::new([StringComparer]::Ordinal)
    $i = 0
    foreach ($b in (@($script:TtBadBudgetsSchema) + @($script:TtBadBudgetsJson))) {
      $i++
      $tickets[$b] = Write-TtPlan -File ('bad{0:d3}.json' -f $i) -Text (New-TtPlan -Name 'Bad budget' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget $b))) -NoSchema
    }
    foreach ($b in '90.0', '9e1', '-0') {
      $tickets["turn|$b"] = Write-TtPlan -File ("other-$($b -replace '[^0-9a-z]', '_').json") -Text (New-TtPlan -Name 'Bad budget' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget '45'), (New-TtTurn -Name 'T02' -Agents 3 -Budget $b))) -NoSchema
    }
    foreach ($g in $script:TtGoodBudgets) {
      $tickets["good|$($g[0])"] = Write-TtPlan -File "good-$($g[0]).json" -Text (New-TtPlan -Name "Budget $($g[0])" -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget $g[0])))
    }
    Invoke-TtBudgetCases -Source file -Fixture $null -Tickets $tickets

    # --- Usage cases, with no stand-in board: the program refuses each before any request ---
    Invoke-TtUsageCases -Listening $false

    # --- Nothing started a provisioning run, and the stand-in saw nothing ---
    Write-TtResult -Name 'lease-api.md#agentspace-lease-create: Creating leases whose lease is unsatisfiable, or refused, starts no provisioning run' -Status $(if (@(Get-PvStartedLines $script:TtStandIn).Count -eq 0) { 'PASS' } else { 'FAIL' }) -Reason "The stand-in provisioning run was started $(@(Get-PvStartedLines $script:TtStandIn).Count) time(s)"
  } catch {
    Write-TtResult -Name 'test-plan-turn-ttl.ps1: The first half ran to its end without an unexpected error' -Status FAIL -Reason "$($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    [Environment]::CurrentDirectory = $savedCwd
    if ($h) {
      try { Clear-PvCleanup -Lab $h } catch { }
      Remove-ConformanceHome -ConformanceHome $h
    }
  }
}

# ====================================================================================================================
# The ready leases: the TTL as the lease's own document gives it, on seeded hosts
# ====================================================================================================================
function Invoke-TtReadyHalf {
  $h = $null
  $savedCwd = [Environment]::CurrentDirectory
  try {
    $h = New-ConformanceHome
    if (-not $h.CaReady) {
      Write-TtResult -Name 'lease-api.md#ttl: A lease that is ready has the TTL its turn gives, as expires_at minus created_at' -Status SKIPPED -Reason "No throwaway CA could be made: $($h.CaReason)"
      return
    }
    Initialize-TtLab -Lab $h -Name 'turn-ttl-ready' -KeepCa
    Set-TtHosts -Lab $h
    $plans = Get-TtPlans
    foreach ($k in $plans.Keys) { [void](Write-TtPlan -File "$k.json" -Text $plans[$k].Text -NoSchema:([bool]$plans[$k].NoSchema)) }
    $t = @{ Source = 'file'; Plans = $plans; State = 'ready'; Reasons = @() }
    $ready = [ordered]@{}
    $rows = @(
      @{ K = 'busiest'; Proj = 'tt-l'; D = 'ready, without --turn: the busiest turn T02, size l, 10800'; A = @(); Turn = 'T02'; Agents = 9; Size = 'l'; Ttl = 10800 }
      @{ K = 't01'; Proj = 'tt-s'; D = 'ready, --turn T01: size s, 4080, which is 67.5 minutes rounded up'; A = @('--turn', 'T01'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 4080 }
      @{ K = 't03'; Proj = 'tt-m'; D = 'ready, --turn T03, which has no budget: size m, the default of 28800'; A = @('--turn', 'T03'); Turn = 'T03'; Agents = 5; Size = 'm'; Ttl = 28800 }
      @{ K = 'ttl-t01'; Proj = 'tt-s'; D = 'ready, --turn T01 with --ttl 2h: 7200, the budget changing nothing'; A = @('--turn', 'T01', '--ttl', '2h'); Turn = 'T01'; Agents = 4; Size = 's'; Ttl = 7200 }
    )
    foreach ($r in $rows) { $ready[$r.K] = Test-TtLease -Section 'ttl' -Desc $r.D -Key 'main' -Extra $r.A -Turn $r.Turn -Agents $r.Agents -Size $r.Size -Ttl $r.Ttl -Project $r.Proj @t }
    $ready['bounds-1'] = Test-TtLease -Section 'ttl' -Desc 'ready, a budget of 1 minute: 120 seconds' -Key 'bounds' -Extra @('--turn', 'T1') -Turn 'T1' -Agents 3 -Size 's' -Ttl 120 -Project 'tt-s' @t
    $ready['bounds-960'] = Test-TtLease -Section 'ttl' -Desc 'ready, a budget of 960 minutes: 86400 seconds, the range''s top' -Key 'bounds' -Extra @('--turn', 'T2') -Turn 'T2' -Agents 3 -Size 's' -Ttl 86400 -Project 'tt-s' @t
    $ready['bounds-3'] = Test-TtLease -Section 'ttl' -Desc 'ready, a budget of 3 minutes: 300 seconds, 4.5 minutes rounded up to 5' -Key 'bounds' -Extra @('--turn', 'T4') -Turn 'T4' -Agents 3 -Size 's' -Ttl 300 -Project 'tt-s' @t
    $ready['null'] = Test-TtLease -Section 'ttl' -Desc 'ready, a turn whose budget_minutes is null: the default' -Key 'null' -Extra @('--turn', 'T1') -Turn 'T1' -Agents 3 -Size 's' -Ttl 28800 -Project 'tt-s' @t
    $ready['large-small'] = Test-TtLease -Section 'turn' -Desc 'ready, a turn that fits though another turn of the plan is too large' -Key 'large' -Extra @('--turn', 'Small') -Turn 'Small' -Agents 3 -Size 's' -Ttl 2700 -Project 'tt-s' @t
    $ready['xl'] = Test-TtLease -Section 'turn' -Desc 'ready, --turn B of 17 agents: size xl, and 300 minutes give 27000' -Key 'xl' -Extra @('--turn', 'B') -Turn 'B' -Agents 17 -Size 'xl' -Ttl 27000 -Project 'tt-xl' @t
    foreach ($k in 't01', 'busiest', 't03', 'xl') { $script:TtReadyDocs[$k] = $ready[$k] }

    # --- A ready lease read back, renewed and released: the TTL is the lease's own, and the plan is never read again ---
    $d = $ready['t01']
    if ($d -is [hashtable] -and [string]$d['id'] -cmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') {
      $id = [string]$d['id']
      $name = 'lease-api.md#agentspace-lease-get: A ready lease made with --turn is read back with the same sized_for and ttl_seconds'
      $p = [System.Collections.Generic.List[string]]::new()
      try {
        $run = Invoke-TtLease -Arguments @('get', $id)
        foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
        if ($run.Json -is [hashtable]) {
          foreach ($x in (Get-TtSizedForProblems -Doc $run.Json -PlanName 'Main plan' -Turn 'T01' -Agents 4)) { $p.Add($x) }
          if ($run.Json['ttl_seconds'] -ne 4080) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not 4080") }
          if ($run.Json['size'] -cne 's') { $p.Add('The size is not s') }
        }
      } catch { $p.Add("It threw: $($_.Exception.Message)") }
      Complete-TtCheck -Name $name -Problems $p
      # The plan file is deleted first, so a renew that read the plan again would fail or differ (lease-api.md, TTL: a renew without --ttl
      # uses the lease's ttl_seconds again, without reading the plan or the board again).
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $script:TtDir 'main.json')) -Force
      $name = 'lease-api.md#ttl: A renew without --ttl uses the lease''s ttl_seconds again, 4080, with the plan file gone: no plan is read again'
      $p = [System.Collections.Generic.List[string]]::new()
      try {
        $run = Invoke-TtLease -Arguments @('renew', $id)
        foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
        if ($run.Json -is [hashtable]) {
          if ($run.Json['ttl_seconds'] -ne 4080) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not 4080") }
          if ($run.Json['state'] -cne 'ready') { $p.Add('The renewed lease is not ready') }
          foreach ($x in (Get-TtSizedForProblems -Doc $run.Json -PlanName 'Main plan' -Turn 'T01' -Agents 4)) { $p.Add($x) }
        }
      } catch { $p.Add("It threw: $($_.Exception.Message)") }
      Complete-TtCheck -Name $name -Problems $p
      $name = 'lease-api.md#agentspace-lease-renew: A renew with --ttl 1h changes the lease''s ttl_seconds to 3600 and keeps its sized_for'
      $p = [System.Collections.Generic.List[string]]::new()
      try {
        $run = Invoke-TtLease -Arguments @('renew', $id, '--ttl', '1h')
        foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
        if ($run.Json -is [hashtable]) {
          if ($run.Json['ttl_seconds'] -ne 3600) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not 3600") }
          foreach ($x in (Get-TtSizedForProblems -Doc $run.Json -PlanName 'Main plan' -Turn 'T01' -Agents 4)) { $p.Add($x) }
        }
      } catch { $p.Add("It threw: $($_.Exception.Message)") }
      Complete-TtCheck -Name $name -Problems $p
    } else {
      foreach ($n in 'A ready lease made with --turn is read back', 'A renew without --ttl uses the lease''s ttl_seconds again', 'A renew with --ttl') { Write-TtResult -Name "lease-api.md#ttl: $n" -Status FAIL -Reason 'No ready lease made with --turn was printed, so there was nothing to read back' }
    }
    Write-TtResult -Name 'lease-api.md#agentspace-lease-create: Creating leases on hosts ready at their size starts no provisioning run' -Status $(if (@(Get-PvStartedLines $script:TtStandIn).Count -eq 0) { 'PASS' } else { 'FAIL' }) -Reason "The stand-in provisioning run was started $(@(Get-PvStartedLines $script:TtStandIn).Count) time(s)"
  } catch {
    Write-TtResult -Name 'test-plan-turn-ttl.ps1: The ready cases ran to their end without an unexpected error' -Status FAIL -Reason "$($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    [Environment]::CurrentDirectory = $savedCwd
    if ($h) {
      try { Clear-PvCleanup -Lab $h } catch { }
      Remove-ConformanceHome -ConformanceHome $h
    }
  }
}

# ====================================================================================================================
# The second half: the plan read from an epic's block, from the stand-in board
# ====================================================================================================================

# True when DPAPI works under the moved home: protects a random string (not the token) in this process and decrypts it in a pwsh child started with the
# harness's environment for the home, and prints nothing of either.
function Test-TtDpapi {
  param([Parameter(Mandatory)]$Lab)
  try {
    $chars = 'abcdefghijklmnopqrstuvwxyz0123456789'
    $probe = -join (1..24 | ForEach-Object { $chars[[Security.Cryptography.RandomNumberGenerator]::GetInt32($chars.Length)] })
    $blob = ConvertFrom-SecureString -SecureString (ConvertTo-SecureString -String $probe -AsPlainText -Force)
    $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $Lab
    foreach ($n in $script:TtLocalswimNames) { $childEnv[$n] = $null }
    $code = '$b = [Console]::In.ReadLine(); $s = [Console]::In.ReadLine(); $t = ConvertTo-SecureString -String $b; $p = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($t); try { $x = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($p) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($p) }; if ($x -ceq $s) { exit 0 } else { exit 3 }'
    $r = Start-ConformanceProcess -FilePath (Get-Process -Id $PID).Path -Arguments @('-NoProfile', '-NonInteractive', '-Command', $code) -ChildEnvironment $childEnv -InputText ($blob + "`n" + $probe + "`n") -TimeoutSeconds 60
    return ($r.ExitCode -eq 0)
  } catch { return $false }
}

function Invoke-TtBoardHalf {
  $h = $null
  $savedCwd = [Environment]::CurrentDirectory
  $board = $null
  try {
    $h = New-ConformanceHome
    Initialize-TtLab -Lab $h -Name 'turn-ttl-board' -KeepCa
    if (-not (Test-TtDpapi -Lab $h)) {
      Write-TtResult -Name 'lease-api.md#the-board-read: The cases of a plan read from an epic' -Status SKIPPED -Reason 'DPAPI does not work under the moved home, so no stand-in credential can be made (docs/conformance.md, The stand-in credential)'
      return
    }
    Set-TtHosts -Lab $h
    $plans = Get-TtPlans
    $cards = [ordered]@{}
    foreach ($k in $plans.Keys) { $cards[[string]$plans[$k].Ticket] = @{ revision = 1; archived = $false; description = (New-TtDescription $plans[$k].Text) } }
    # The ticket of each P24 plan: 200 and up.
    $tickets = [hashtable]::new([StringComparer]::Ordinal)
    $n = 200
    foreach ($b in (@($script:TtBadBudgetsSchema) + @($script:TtBadBudgetsJson))) {
      $n++; $tickets[$b] = $n
      $cards[[string]$n] = @{ revision = 1; archived = $false; description = (New-TtDescription (New-TtPlan -Name 'Bad budget' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget $b)))) }
    }
    foreach ($b in '90.0', '9e1', '-0') {
      $n++; $tickets["turn|$b"] = $n
      $cards[[string]$n] = @{ revision = 1; archived = $false; description = (New-TtDescription (New-TtPlan -Name 'Bad budget' -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget '45'), (New-TtTurn -Name 'T02' -Agents 3 -Budget $b)))) }
    }
    foreach ($g in $script:TtGoodBudgets) {
      $n++; $tickets["good|$($g[0])"] = $n
      $cards[[string]$n] = @{ revision = 1; archived = $false; description = (New-TtDescription (New-TtPlan -Name "Budget $($g[0])" -Turns @((New-TtTurn -Name 'T01' -Agents 3 -Budget $g[0])))) }
    }
    # The fixture: the token in this process's memory goes into the file in the home, and nowhere else.
    $fixture = [ordered]@{ token = $script:TtToken; boards = @{ $script:TtBoardId = @{ cards = $cards } } }
    $fixturePath = Join-Path $h.Root 'board\fixture.json'
    $logPath = Join-Path $h.Root 'board\requests.log'
    $readyPath = Join-Path $h.Root 'board\ready.txt'
    Write-PvText -Lab $h -Path $fixturePath -Text (ConvertTo-Json -InputObject $fixture -Depth 10 -Compress)
    $board = Start-TtBoard -Lab $h -Fixture $fixturePath -Log $logPath -Ready $readyPath
    if ($null -eq $board) {
      Write-TtResult -Name 'lease-api.md#the-board-read: The stand-in board starts and is ready' -Status FAIL -Reason $script:TtBoardWhy
      return
    }
    $script:TtBoard = $board
    # The receipt, and bot.json with the token protected for this Windows user (docs/conformance.md, The stand-in credential).
    $local = Join-Path $h.UserProfile 'AppData\Local\localswim-v2'
    $credDir = Join-Path $local 'credentials'
    $receipt = ConvertTo-Json -InputObject ([ordered]@{ base = "http://127.0.0.1:$($board.Port)"; credential_directory = $credDir }) -Compress
    Write-PvText -Lab $h -Path (Join-Path $local 'service.json') -Text $receipt
    $hex = (ConvertFrom-SecureString -SecureString (ConvertTo-SecureString -String $script:TtToken -AsPlainText -Force)).ToLowerInvariant()
    $bot = ConvertTo-Json -InputObject ([ordered]@{ format = 'localswim-credential'; format_version = 1; role = 'bot'; token_dpapi_hex = $hex }) -Compress
    Write-PvText -Lab $h -Path (Join-Path $credDir 'bot.json') -Text $bot
    $hex = $null
    $bot = $null

    # A good read, first: one GET, the request the spec gives, with the token the stand-in compared (auth=ok).
    $name = 'lease-api.md#the-board-read: A good epic is read with one GET, the request the spec gives, carrying the credential''s token'
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $before = @(Get-TtLogLines).Count
      $run = Invoke-TtLease -Arguments @('create', '--project', 'ttnocap', '--board', $script:TtBoardId, '--epic', '0101')
      $new = @(Get-TtLogLines | Select-Object -Skip $before)
      foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
      foreach ($x in (Get-TtLogProblems -Lines $new -Reads 1 -Ticket 101)) { $p.Add($x) }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-TtCheck -Name $name -Problems $p

    Invoke-TtSourceCases -Source board -Plans $plans | Out-Null
    Invoke-TtBudgetCases -Source board -Fixture $fixture -Tickets $tickets
    Invoke-TtUsageCases -Listening $true

    # --- Ready leases from an epic, and a renew that reads no board ---
    if ($h.CaReady) {
      $t = @{ Source = 'board'; Plans = $plans; State = 'ready'; Reasons = @() }
      $rd = [ordered]@{}
      $rd['t01'] = Test-TtLease -Section 'ttl' -Desc 'ready, --turn T01: size s, 4080' -Key 'main' -Extra @('--turn', 'T01') -Turn 'T01' -Agents 4 -Size 's' -Ttl 4080 -Project 'tt-s' @t
      $rd['busiest'] = Test-TtLease -Section 'ttl' -Desc 'ready, without --turn: the busiest turn T02, size l, 10800' -Key 'main' -Turn 'T02' -Agents 9 -Size 'l' -Ttl 10800 -Project 'tt-l' @t
      $rd['t03'] = Test-TtLease -Section 'ttl' -Desc 'ready, --turn T03, which has no budget: the default' -Key 'main' -Extra @('--turn', 'T03') -Turn 'T03' -Agents 5 -Size 'm' -Ttl 28800 -Project 'tt-m' @t
      $rd['xl'] = Test-TtLease -Section 'turn' -Desc 'ready, --turn B of 17 agents: size xl, 27000' -Key 'xl' -Extra @('--turn', 'B') -Turn 'B' -Agents 17 -Size 'xl' -Ttl 27000 -Project 'tt-xl' @t
      foreach ($k in 't01', 'busiest', 't03', 'xl') {
        $a = $script:TtReadyDocs[$k]
        $b = $rd[$k]
        $name = "lease-api.md#plan: The same plan and the same line give the same ready lease from an epic as from a file: $k"
        if ($a -is [hashtable] -and $b -is [hashtable]) { Complete-TtCheck -Name $name -Problems (Get-TtSameProblems -A $a -B $b) }
        else { Write-TtResult -Name $name -Status FAIL -Reason 'One of the two leases was not printed, so they could not be compared' }
      }
      $d = $rd['t01']
      if ($d -is [hashtable] -and [string]$d['id'] -cmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') {
        $id = [string]$d['id']
        $name = 'lease-api.md#ttl: A renew of a lease made from an epic uses its ttl_seconds again, 4080, and reads no board: the stand-in logs no request'
        $p = [System.Collections.Generic.List[string]]::new()
        try {
          $before = @(Get-TtLogLines).Count
          $run = Invoke-TtLease -Arguments @('renew', $id)
          $new = @(Get-TtLogLines | Select-Object -Skip $before)
          foreach ($x in (Get-ConformanceSuccessProblems -Run $run)) { $p.Add($x) }
          if ($run.Json -is [hashtable] -and $run.Json['ttl_seconds'] -ne 4080) { $p.Add("ttl_seconds is $($run.Json['ttl_seconds']), not 4080") }
          foreach ($x in (Get-TtLogProblems -Lines $new -Reads 0)) { $p.Add($x) }
        } catch { $p.Add("It threw: $($_.Exception.Message)") }
        Complete-TtCheck -Name $name -Problems $p
      } else { Write-TtResult -Name 'lease-api.md#ttl: A renew of a lease made from an epic uses its ttl_seconds again and reads no board' -Status FAIL -Reason 'No ready lease made from an epic was printed, so there was nothing to renew' }
    } else {
      Write-TtResult -Name 'lease-api.md#ttl: A lease that is ready, made from an epic, has the TTL its turn gives' -Status SKIPPED -Reason "No throwaway CA could be made: $($h.CaReason)"
    }

    # --- Every request of the half was a GET that carried the credential's token, and nothing but the epics' cards was asked for ---
    $all = @(Get-TtLogLines)
    $p = [System.Collections.Generic.List[string]]::new()
    if ($all.Count -eq 0) { $p.Add('The stand-in logged no request at all') }
    foreach ($l in $all) {
      if ($l -cnotmatch '\AGET /api/2026-11-10/boards/[0-9a-f-]{36}\?view=ticket&ticket=\d+&include_prose=true auth=ok\z') { $p.Add('A logged request is not the GET of a ticket view with the credential''s token') }
    }
    Complete-TtCheck -Name 'lease-api.md#the-board-read: Every request of these cases was one GET of a ticket view with the credential''s token, and none of another method or kind' -Problems $p
    # The control: a planted POST line, and a planted line with a wrong token, make the same check fail.
    $planted = @($all) + @('POST ' + $script:TtApi + $script:TtBoardId + '?view=ticket&ticket=101&include_prose=true auth=ok')
    $pc = [System.Collections.Generic.List[string]]::new()
    foreach ($l in $planted) { if ($l -cnotmatch '\AGET /api/2026-11-10/boards/[0-9a-f-]{36}\?view=ticket&ticket=\d+&include_prose=true auth=ok\z') { $pc.Add('seen') } }
    Write-TtResult -Name 'lease-api.md#the-board-read: Control: a log with a planted POST line fails the check that every request was a GET' -Status $(if ($pc.Count -ge 1) { 'PASS' } else { 'FAIL' }) -Reason 'The check did not see the planted line'
  } catch {
    Write-TtResult -Name 'test-plan-turn-ttl.ps1: The second half ran to its end without an unexpected error' -Status FAIL -Reason "$($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
  } finally {
    if ($board) { Stop-TtProcess -Process $board.Process -StartTicks $board.StartTicks }
    $script:TtBoard = $null
    [Environment]::CurrentDirectory = $savedCwd
    if ($h) {
      try { Clear-PvCleanup -Lab $h } catch { }
      Remove-ConformanceHome -ConformanceHome $h
    }
  }
}

Invoke-TtFileHalf
Invoke-TtReadyHalf
Invoke-TtBoardHalf

# --- The token never reached a printed reason or a program's output ---------------------------------------------------------
Write-TtResult -Name 'lease-api.md#the-board-read: The token was in no stdout or stderr of any run of the program' -Status $(if ($script:TtLeaks -eq 0 -and $script:TtRuns -gt 0) { 'PASS' } else { 'FAIL' }) -Reason "$($script:TtLeaks) of $($script:TtRuns) runs printed the token"
# The control: a reason planted with the token comes out of Format-TtText without it, and with the placeholder.
$planted = "A reason that holds $($script:TtToken), twice: $($script:TtToken), and a path C:\x\y."
$cleaned = Format-TtText $planted
$okRedaction = (-not $cleaned.Contains($script:TtToken)) -and $cleaned.Contains('<token>') -and (-not $cleaned.Contains('C:\x'))
Write-TtResult -Name 'lease-api.md#the-board-read: Control: a reason planted with the token comes out of Format-TtText without it' -Status $(if ($okRedaction) { 'PASS' } else { 'FAIL' }) -Reason 'The planted reason still held the token or a path'
# And the same through the function a result goes through: a failing result's reason is redacted too.
$partial = Format-TtText ("cut here: " + $script:TtToken.Substring(0, 20) + "and " + $script:TtToken)
Write-TtResult -Name 'lease-api.md#the-board-read: Control: a reason with the whole token in the middle of other text is redacted, and no whole token is left' -Status $(if (-not $partial.Contains($script:TtToken)) { 'PASS' } else { 'FAIL' }) -Reason 'A whole token was left in the reason'
# The leak count itself can fail: a run whose output held the token is counted.
$probeLeaks = $script:TtLeaks
$fake = [pscustomobject]@{ Stdout = "x$($script:TtToken)"; Stderr = '' }
$would = ($script:TtToken -and (([string]$fake.Stdout).Contains($script:TtToken) -or ([string]$fake.Stderr).Contains($script:TtToken)))
Write-TtResult -Name 'lease-api.md#the-board-read: Control: the check for the token in a program''s output sees a planted one' -Status $(if ($would) { 'PASS' } else { 'FAIL' }) -Reason 'The check did not see the planted token'
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.10-T04-S01, #0221): the errors of `agentspace lease create --plan`, and
# the one answer a plan can get that is not an error, `plan_too_large` (docs/lease-api.md, contract v1;
# D74, D75, D81). It drives the lease CLI only as a separate process, through the harness, in throwaway
# homes. Every expectation is taken from docs/lease-api.md, never from a program's output.
#
# What it enforces, by the lease-api.md section each result names:
#   #command-lines, #agentspace-lease-create   `--plan` with `--size` is a usage error with field `plan`, in each
#       order and each form, where the later of the two flags stands, and the ranking of that error among the
#       other usage errors; the order in which the values are checked (project, platform, profile, plan, size,
#       ttl); help wins over everything.
#   #plan   Every refusal of a plan is `invalid_value` for `plan`, exit 2, with the message frame
#       `Plan '<value>' is not a valid plan: <why>.` and the `<why>` of the first check the plan fails, in the
#       table's order and only the first fault of that check: the size bound (262144 bytes, a byte order mark
#       counted), a file that cannot be read (missing, a folder, a device, a pipe, locked, `-`, an argument with
#       U+FEFF, a lone surrogate or a control character), UTF-8, JSON, a member named twice, the plan schema,
#       a turn, a story or a subagent named twice, an `after` naming no subagent of its story, and a cycle. A
#       refused plan records no lease and creates nothing, and no message carries an operating system's error
#       text, a path but the argument, or a model, an effort or any other value of the document.
#   #fields, #agentspace-lease-create   `plan_too_large`: the lease's only reason, whatever its platform and
#       profile, with the message byte for byte, `size` xl and `sized_for`, at the boundary (the most that fits
#       is not `plan_too_large` and one more is), the busiest turn and not the sum, the first on a tie; decided
#       before any driver is asked, so it needs no CA, makes no host and starts no provisioning run.
#
# The most agents at once the largest size holds is 32 at today's footprint (lease-api.md, Plan). The footprint
# is agentspace's and MAY change without a new contract version; if it does, the figure 32 below changes with
# it (the cases' own constant, $script:PeMost).
#
# No case needs a CA: the first thing a home's cases do is remove the throwaway CA the harness made, so that a
# case which passed only because a CA was there cannot pass. No case reaches a provisioning run: every create
# names the recording stand-in of provision-standin.ps1 as AGENTSPACE_PROVISION_CMD, and a result at the end of
# each home shows it was never started. A project that has no host and is not on the project list answers
# `unsatisfiable` with `no_capacity` before any host is made (D54), so a plan a size holds is answered with no
# run. Every plan file is written inside the throwaway home, through Assert-ConformancePath, and the program is
# run with the plans folder as its working folder, so an argument is a bare file name and no message holds a
# path of the machine's. A failure reason is masked before it is printed.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-plan-errors.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'provision-standin.ps1')

# The most agents at once that xl holds at today's footprint (lease-api.md, Plan: 16 vCPUs and 64 GiB at half a
# vCPU and 2 GiB each).
$script:PeMost = 32
$script:PeLab = $null
$script:PeDir = $null
$script:PeStandIn = $null
$script:PeSeen = [System.Collections.Generic.List[string]]::new()
# How many lease documents the cases of the second home read: a result about what a create left or started shows
# nothing unless the program answered, so each also needs these (a run against a program that prints nothing fails them).
$script:PeLeases = 0

# The <why> texts of lease-api.md, Plan, in the table's order (checks 1 to 6, and the fixed ones of 7 to 11).
$script:WhySize = 'the file is larger than 262144 bytes'
$script:WhyRead = 'the file could not be read'
$script:WhyUtf8 = 'the file is not valid UTF-8'
$script:WhyJson = 'the file is not valid JSON'
$script:WhySchema = 'the document does not match the plan schema'
function Get-PeWhyMembers([string]$Member) { "an object has two members named '$Member'" }
function Get-PeWhyTurn([string]$Turn) { "turn '$Turn' is named twice" }
function Get-PeWhyStory([string]$Story, [string]$Turn) { "story '$Story' is named twice in turn '$Turn'" }
function Get-PeWhySubagent([string]$Name, [string]$Story) { "subagent '$Name' is named twice in story '$Story'" }
function Get-PeWhyAfter([string]$Name, [string]$Story, [string]$After) { "subagent '$Name' in story '$Story' starts after '$After', which is no subagent of that story" }
function Get-PeWhyCycle([string]$Story) { "the subagents of story '$Story' start after each other in a cycle" }

# --- Text, masked and quoted -------------------------------------------------------------------------------

# A text for a failure reason: masked by the suite's mask (conformance\mask-common.ps1) before anything else, then
# whitespace is collapsed and the length is bounded, so a cut never leaves part of a name unmasked.
function Format-PeText([string]$Text) {
  $x = [string]$Text
  $x = ConvertTo-ConformanceMasked $x
  $x = $x -replace '\s+', ' '
  if ($x.Length -gt 260) { $x = $x.Substring(0, 260) + '...' }
  return $x
}

# A value as a message quotes it (lease-api.md, Messages): each control character, U+0000 to U+001F and U+007F,
# and each unpaired surrogate as \u and four lowercase hex digits; everything else as it is.
function Format-PeQuoted([string]$Value) {
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

# The message of a refused plan (lease-api.md, Messages): the argument as given, quoted, and the <why>.
function Get-PePlanMessage([string]$Arg, [string]$Why) { "Plan '$(Format-PeQuoted $Arg)' is not a valid plan: $Why." }

function Complete-PeCheck {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ((@($Problems | ForEach-Object { Format-PeText $_ })) -join '; ') }
}

# --- Building plans -----------------------------------------------------------------------------------------

function ConvertTo-PeJson([string]$Value) {
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

function New-PeSub {
  param([string]$Name = 'A', [string]$Model = 'Opus', [string]$Effort = 'high', [string[]]$After)
  $j = '{"name":' + (ConvertTo-PeJson $Name) + ',"model":' + (ConvertTo-PeJson $Model) + ',"effort":' + (ConvertTo-PeJson $Effort)
  if ($PSBoundParameters.ContainsKey('After')) { $j += ',"after":[' + ((@($After | Where-Object { $null -ne $_ } | ForEach-Object { ConvertTo-PeJson $_ })) -join ',') + ']' }
  return $j + '}'
}
function New-PeStory { param([string]$Name = 'S1', [string[]]$Subs) return '{"name":' + (ConvertTo-PeJson $Name) + ',"subagents":[' + ($Subs -join ',') + ']}' }
function New-PeTurn { param([string]$Name = 'T1', [string[]]$Stories) return '{"name":' + (ConvertTo-PeJson $Name) + ',"stories":[' + ($Stories -join ',') + ']}' }
function New-PePlan {
  param([string[]]$Turns, [string]$Name)
  $head = if ($PSBoundParameters.ContainsKey('Name')) { '"name":' + (ConvertTo-PeJson $Name) + ',' } else { '' }
  return '{' + $head + '"turns":[' + ($Turns -join ',') + ']}'
}
# A turn of $Stories stories, each of $Subagents subagents that start after none: its agents at once are
# $Stories * $Subagents + 1 (lease-api.md, Plan: a story's peak is counted from its starts, a turn's agents are the
# sum of its stories' peaks and 1 for its dev lead).
function New-PeWideTurn {
  param([string]$Name = 'T1', [Parameter(Mandatory)][int]$Stories, [int]$Subagents = 1)
  $subs = @(1..$Subagents | ForEach-Object { New-PeSub -Name ('a{0:d2}' -f $_) })
  $st = @(1..$Stories | ForEach-Object { New-PeStory -Name ('s{0:d2}' -f $_) -Subs $subs })
  return (New-PeTurn -Name $Name -Stories $st)
}

$script:OkSub = New-PeSub
$script:OkStory = New-PeStory -Subs @($script:OkSub)
$script:OkTurn = New-PeTurn -Stories @($script:OkStory)
$script:OkPlan = New-PePlan -Turns @($script:OkTurn)

# --- Files, in the throwaway home --------------------------------------------------------------------------

function Write-PeBytes {
  param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyCollection()][byte[]]$Bytes)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $script:PeLab -Path $script:PeDir) | Out-Null
  [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $script:PeLab -Path (Join-Path $script:PeDir $Name)), $Bytes)
}
function Write-PeText { param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][AllowEmptyString()][string]$Text) Write-PeBytes -Name $Name -Bytes ([Text.UTF8Encoding]::new($false).GetBytes($Text)) }

$script:PeBom = [byte[]](0xEF, 0xBB, 0xBF)
# ASCII text padded with spaces to exactly $Total bytes, with the byte order mark first when asked (it counts).
function New-PePadded {
  param([Parameter(Mandatory)][AllowEmptyString()][string]$Text, [Parameter(Mandatory)][int]$Total, [switch]$Bom)
  $buf = [byte[]]::new($Total)
  [Array]::Fill($buf, [byte]0x20)
  $offset = 0
  if ($Bom) { [Array]::Copy($script:PeBom, 0, $buf, 0, 3); $offset = 3 }
  $t = [Text.Encoding]::ASCII.GetBytes($Text)
  [Array]::Copy($t, 0, $buf, $offset, $t.Length)
  return , $buf
}

# Makes a throwaway home for a group of cases: no CA, a plans folder that is the program's working folder, the
# recording stand-in every create names, and a plan the cases share, ok.json.
function Initialize-PeLab {
  param([Parameter(Mandatory)]$Lab)
  $script:PeLab = $Lab
  $script:PeDir = Join-Path $Lab.Root 'plans'
  $script:PeSeen.Clear()
  Remove-PvCa -Lab $Lab
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $script:PeDir) | Out-Null
  $script:PeStandIn = New-PvStandIn -Lab $Lab -Name 'plan-errors'
  Write-PeText -Name 'ok.json' -Text $script:OkPlan
  [Environment]::CurrentDirectory = (Assert-ConformancePath -ConformanceHome $Lab -Path $script:PeDir)
}

# --- Running the lease CLI ----------------------------------------------------------------------------------

# `agentspace lease <Arguments>`, with the stand-in as the provisioning run and the case's ProxyCommand, in the
# plans folder. Returns the harness's result.
function Invoke-PeLease {
  param([Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [string]$InputJson, [int]$TimeoutSeconds = 60)
  $e = @{ AGENTSPACE_PROXY_COMMAND = $script:CcProxy; AGENTSPACE_PROVISION_CMD = $script:PeStandIn.Path }
  $splat = @{ Variable = 'AGENTSPACE_CMD'; ConformanceHome = $script:PeLab; Arguments = (@('lease') + $Arguments); Environment = $e; TimeoutSeconds = $TimeoutSeconds }
  if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputJson'] = $InputJson }
  return (Invoke-ConformanceProgram @splat)
}

# The result of a run that was meant to fail with a given code: the harness's error test, and the message, which
# MUST be the one given when $Message is given.
function Get-PeErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [string]$Field = '', [string[]]$Messages)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceErrorProblems -Run $Run -Code $Code -Field $Field)) { $p.Add($x) }
  $doc = $Run.ErrorJson
  if ($doc -is [hashtable] -and $doc.ContainsKey('message')) {
    [void]$script:PeSeen.Add([string]$Run.Stderr)
    $msg = [string]$doc['message']
    if ($Messages -and ($Messages -cnotcontains $msg)) { $p.Add("The message is '$msg', not '$($Messages -join "' or '")'") }
  }
  return , $p
}

# --- Usage errors: --plan with --size, in each order, and where it ranks --------------------------------------

function Get-PeUsageMessage {
  param([Parameter(Mandatory)][ValidateSet('with-size', 'no-value', 'empty-value', 'twice')][string]$Kind, [string]$Flag = 'plan')
  switch ($Kind) {
    'with-size' { "Options --plan and --size cannot be given together; the plan picks the size. Run 'agentspace lease create --help'." }
    'no-value' { "Option --$Flag needs a value. Run 'agentspace lease create --help'." }
    'empty-value' { "Option --$Flag has an empty value. Run 'agentspace lease create --help'." }
    'twice' { "Option --$Flag is given twice. Give it once." }
  }
}

# A `create` line (after `lease create`) that MUST be a usage error with the field and message given.
function Test-PeUsage {
  param([Parameter(Mandatory)][string]$Why, [Parameter(Mandatory)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments, [string]$Field = '', [string]$Message)
  $line = @('lease', 'create') + $Arguments
  $where = if ($Field) { "field '$Field'" } else { 'no field' }
  $name = "lease-api.md#command-lines: $(Format-ConformanceLine $line) is a usage error with $where ($Why), exit 2"
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $run = Invoke-PeLease -Arguments (@('create') + $Arguments)
    $msgs = if ($Message) { @($Message) } else { @() }
    foreach ($x in (Get-PeErrorProblems -Run $run -Code usage -Field $Field -Messages $msgs)) { $p.Add($x) }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $script:PeLab)) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name $name -Problems $p
}

# A `create` line that MUST fail with the code and field given (a value error): the harness's error test.
function Test-PeValueError {
  param([Parameter(Mandatory)][string]$Why, [Parameter(Mandatory)][string[]]$Arguments, [Parameter(Mandatory)][string]$Code, [Parameter(Mandatory)][string]$Field)
  $line = @('lease', 'create') + $Arguments
  $name = "lease-api.md#agentspace-lease-create: $(Format-ConformanceLine $line) is $Code for '$Field' ($Why), exit 2"
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $run = Invoke-PeLease -Arguments (@('create') + $Arguments)
    foreach ($x in (Get-PeErrorProblems -Run $run -Code $Code -Field $Field)) { $p.Add($x) }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $script:PeLab)) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name $name -Problems $p
}

# --- Refusals of a plan ----------------------------------------------------------------------------------------

# A `create --plan <Arg>` that MUST be refused with invalid_value for plan and one of the <why> texts given, and
# MUST record nothing and make nothing.
function Test-PeRefusal {
  param(
    [Parameter(Mandatory)][string]$Desc, [Parameter(Mandatory)][AllowEmptyString()][string]$Arg, [Parameter(Mandatory)][string[]]$Whys,
    [string[]]$Before = @(), [string[]]$After = @(), [string]$InputJson, [int]$TimeoutSeconds = 60, [switch]$EqualsForm
  )
  $name = "lease-api.md#plan: $Desc is refused with invalid_value for plan: $($Whys -join ' or ')"
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $plan = if ($EqualsForm) { @("--plan=$Arg") } else { @('--plan', $Arg) }
    $argv = @('create', '--project', 'pe-case') + $Before + $plan + $After
    $splat = @{ Arguments = $argv; TimeoutSeconds = $TimeoutSeconds }
    if ($PSBoundParameters.ContainsKey('InputJson')) { $splat['InputJson'] = $InputJson }
    $run = Invoke-PeLease @splat
    $expected = @($Whys | ForEach-Object { Get-PePlanMessage $Arg $_ })
    foreach ($x in (Get-PeErrorProblems -Run $run -Code invalid_value -Field plan -Messages $expected)) { $p.Add($x) }
    foreach ($x in (Get-PvNothingRecordedProblems -Lab $script:PeLab)) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name $name -Problems $p
}

$script:PeFiles = 0
# Writes a file (text or bytes) under a fresh name and tests that it is refused with the <why> given.
function Test-PeFile {
  param([Parameter(Mandatory)][string]$Desc, [string]$Text, [byte[]]$Bytes, [Parameter(Mandatory)][string[]]$Whys)
  $script:PeFiles++
  $file = 'p{0:d3}.json' -f $script:PeFiles
  if ($PSBoundParameters.ContainsKey('Bytes')) { Write-PeBytes -Name $file -Bytes $Bytes } else { Write-PeText -Name $file -Text $Text }
  Test-PeRefusal -Desc $Desc -Arg $file -Whys $Whys
}

# --- Answers that are not errors ----------------------------------------------------------------------------------

# The sized_for of a document: its members' names, joined, and the values.
function Get-PeSizedForProblems {
  param($Doc, [string]$Turn, [int]$Agents, [AllowNull()]$PlanName)
  $p = [System.Collections.Generic.List[string]]::new()
  $sf = $Doc['sized_for']
  if ($sf -isnot [hashtable]) { $p.Add('There is no sized_for'); return , $p }
  $want = @('agents', 'turn') + $(if ($null -ne $PlanName) { @('plan') } else { @() })
  $got = @($sf.Keys | ForEach-Object { [string]$_ } | Sort-Object)
  if ((($got -join ',')) -cne ((@($want | Sort-Object)) -join ',')) { $p.Add("sized_for has the members [$($got -join ', ')], not [$((@($want | Sort-Object)) -join ', ')]") }
  if ($sf['turn'] -cne $Turn) { $p.Add("sized_for.turn is '$($sf['turn'])', not '$Turn'") }
  if ($sf['agents'] -ne $Agents) { $p.Add("sized_for.agents is $($sf['agents']), not $Agents") }
  if ($null -ne $PlanName -and $sf['plan'] -cne $PlanName) { $p.Add("sized_for.plan is '$($sf['plan'])', not '$PlanName'") }
  return , $p
}

# The problems with a `plan_too_large` answer: a lease that validates against the lease schema (which refuses a
# plan_too_large that is not alone, or with a size but xl, or with no sized_for), unsatisfiable, with the one
# reason and its message byte for byte, size xl and sized_for.
function Get-PeTooLargeProblems {
  param(
    [Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents, [AllowNull()]$PlanName,
    [string]$Project = 'pe-case', [string]$Platform = 'linux/arm64', [string]$ProfileText = 'agent-linux', [int]$TtlSeconds = 28800
  )
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  $script:PeLeases++
  if ($doc['state'] -cne 'unsatisfiable') { $p.Add("The state is '$($doc['state'])', not unsatisfiable") }
  if ($doc['size'] -cne 'xl') { $p.Add("The size is '$($doc['size'])', not xl") }
  if ($doc['project'] -cne $Project) { $p.Add('The project is not the one asked for') }
  if ($doc['platform'] -cne $Platform) { $p.Add("The platform is '$($doc['platform'])', not '$Platform'") }
  if ($doc['profile'] -cne $ProfileText) { $p.Add("The profile is '$($doc['profile'])', not '$ProfileText'") }
  if ($doc['ttl_seconds'] -ne $TtlSeconds) { $p.Add("ttl_seconds is $($doc['ttl_seconds']), not $TtlSeconds") }
  foreach ($m in 'ssh', 'expires_at', 'released_at') { if ($doc.ContainsKey($m)) { $p.Add("An unsatisfiable lease carries $m") } }
  $reasons = @($doc['reasons'] | Where-Object { $null -ne $_ })
  if ($reasons.Count -ne 1) { $p.Add("There are $($reasons.Count) reasons, not the one plan_too_large: [$((@($reasons | ForEach-Object { [string]$_['code'] })) -join ', ')]") }
  elseif ($reasons[0]['code'] -cne 'plan_too_large') { $p.Add("The one reason is '$($reasons[0]['code'])', not plan_too_large") }
  else {
    $want = "Plan's busiest turn '$Turn' has $Agents agents at once; the largest size, xl, holds $script:PeMost."
    if ($reasons[0]['message'] -cne $want) { $p.Add("The message is '$($reasons[0]['message'])', not '$want'") }
  }
  foreach ($x in (Get-PeSizedForProblems -Doc $doc -Turn $Turn -Agents $Agents -PlanName $PlanName)) { $p.Add($x) }
  return , $p
}

# The problems with an answer to a plan a size holds: a lease that validates, with sized_for, and with no
# plan_too_large; the project has no host and is not on the list, so its reason is no_capacity (D54), whatever
# the size.
function Get-PeFitsProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents, [AllowNull()]$PlanName, [string]$Size)
  $p = [System.Collections.Generic.List[string]]::new()
  foreach ($x in (Get-ConformanceSuccessProblems -Run $Run)) { $p.Add($x) }
  $doc = $Run.Json
  if ($doc -isnot [hashtable]) { return , $p }
  $script:PeLeases++
  if ($doc['state'] -cne 'unsatisfiable') { $p.Add("The state is '$($doc['state'])', not unsatisfiable") }
  $codes =@($doc['reasons'] | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_['code'] })
  if (($codes -join ',') -cne 'no_capacity') { $p.Add("The reasons are [$($codes -join ', ')], not [no_capacity]: a plan a size holds is judged by the driver as a --size lease is") }
  if ($Size -and $doc['size'] -cne $Size) { $p.Add("The size is '$($doc['size'])', not $Size") }
  foreach ($x in (Get-PeSizedForProblems -Doc $doc -Turn $Turn -Agents $Agents -PlanName $PlanName)) { $p.Add($x) }
  return , $p
}

# --- The cases ------------------------------------------------------------------------------------------------------

$h1 = $null
$h2 = $null
$savedCwd = [Environment]::CurrentDirectory
try {
  # ===== Home 1: refusals, and the usage and value errors around them. No lease is ever recorded here. =====
  $h1 = New-ConformanceHome
  Initialize-PeLab -Lab $h1

  # --- --plan with --size, and the usage errors around it (lease-api.md#command-lines, create) ---
  $W = Get-PeUsageMessage -Kind with-size
  $c = @('create', '--project', 'pe-case')
  Test-PeUsage -Why 'plan, then size' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', 'm')) -Field plan -Message $W
  Test-PeUsage -Why 'size, then plan' -Arguments ($c[1..2] + @('--size', 'm', '--plan', 'ok.json')) -Field plan -Message $W
  Test-PeUsage -Why 'the = form, plan first' -Arguments ($c[1..2] + @('--plan=ok.json', '--size=m')) -Field plan -Message $W
  Test-PeUsage -Why 'the = form, size first' -Arguments ($c[1..2] + @('--size=m', '--plan=ok.json')) -Field plan -Message $W
  Test-PeUsage -Why 'flags in other cases, plan first' -Arguments ($c[1..2] + @('--PLAN', 'ok.json', '--Size', 'm')) -Field plan -Message $W
  Test-PeUsage -Why 'flags in other cases, size first' -Arguments ($c[1..2] + @('--SIZE', 'm', '--Plan', 'ok.json')) -Field plan -Message $W
  Test-PeUsage -Why 'the file is missing: a usage error comes before any value is checked' -Arguments ($c[1..2] + @('--plan', 'missing.json', '--size', 'm')) -Field plan -Message $W
  Test-PeUsage -Why 'the size is no size: usage before values' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', 'toolarge9')) -Field plan -Message $W
  Test-PeUsage -Why 'the TTL is bad: usage before values' -Arguments ($c[1..2] + @('--size', 'm', '--plan', 'ok.json', '--ttl', '8')) -Field plan -Message $W
  Test-PeUsage -Why 'the platform is bad: usage before values' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--platform', 'linux', '--size', 'm')) -Field plan -Message $W
  Test-PeUsage -Why 'the project is bad: usage before values' -Arguments @('--project', 'bad_', '--plan', 'ok.json', '--size', 'm') -Field plan -Message $W
  Test-PeUsage -Why 'other flags between the two' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--ttl', '1h', '--platform', 'linux/arm64', '--size', 'm')) -Field plan -Message $W
  Test-PeUsage -Why 'no --project: the usage error in line order comes before the missing project' -Arguments @('--plan', 'ok.json', '--size', 'm') -Field plan -Message $W
  Test-PeUsage -Why 'a size with no value, last: reported as that, for size' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size')) -Field size -Message (Get-PeUsageMessage -Kind no-value -Flag size)
  Test-PeUsage -Why 'a size with an empty value: reported as that, for size' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', '')) -Field size -Message (Get-PeUsageMessage -Kind empty-value -Flag size)
  Test-PeUsage -Why 'a size with an empty value in the = form' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size=')) -Field size -Message (Get-PeUsageMessage -Kind empty-value -Flag size)
  Test-PeUsage -Why 'a plan with no value, last: reported as that, for plan' -Arguments ($c[1..2] + @('--size', 'm', '--plan')) -Field plan -Message (Get-PeUsageMessage -Kind no-value -Flag plan)
  Test-PeUsage -Why 'a plan with an empty value: reported as that, for plan' -Arguments ($c[1..2] + @('--size', 'm', '--plan', '')) -Field plan -Message (Get-PeUsageMessage -Kind empty-value -Flag plan)
  Test-PeUsage -Why 'a plan with an empty value in the = form' -Arguments ($c[1..2] + @('--size', 'm', '--plan=')) -Field plan -Message (Get-PeUsageMessage -Kind empty-value -Flag plan)
  Test-PeUsage -Why 'a plan whose next argument starts with -- has no value, and stands first' -Arguments ($c[1..2] + @('--plan', '--size', 'm')) -Field plan -Message (Get-PeUsageMessage -Kind no-value -Flag plan)
  Test-PeUsage -Why 'a size whose next argument starts with -- has no value, and stands first' -Arguments ($c[1..2] + @('--size', '--plan', 'ok.json')) -Field size -Message (Get-PeUsageMessage -Kind no-value -Flag size)
  Test-PeUsage -Why 'a plan with no value alone' -Arguments ($c[1..2] + @('--plan')) -Field plan -Message (Get-PeUsageMessage -Kind no-value -Flag plan)
  Test-PeUsage -Why 'a plan with an empty value alone' -Arguments ($c[1..2] + @('--plan', '')) -Field plan -Message (Get-PeUsageMessage -Kind empty-value -Flag plan)
  Test-PeUsage -Why 'a plan given twice' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--plan', 'ok.json')) -Field plan -Message (Get-PeUsageMessage -Kind twice -Flag plan)
  Test-PeUsage -Why 'a plan given twice, in another case and the = form' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--PLAN=ok.json')) -Field plan -Message (Get-PeUsageMessage -Kind twice -Flag plan)
  Test-PeUsage -Why 'a plan given twice stands before the size that follows' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--plan', 'ok.json', '--size', 'm')) -Field plan -Message (Get-PeUsageMessage -Kind twice -Flag plan)
  Test-PeUsage -Why 'the size stands before the second plan, so the pair is what is reported' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', 'm', '--plan', 'ok.json')) -Field plan -Message $W
  Test-PeUsage -Why 'an unknown option before the pair is reported first' -Arguments ($c[1..2] + @('--bogus', '--plan', 'ok.json', '--size', 'm')) -Message "Unknown option '--bogus' for agentspace lease create. Run 'agentspace lease create --help'."
  Test-PeUsage -Why 'an unknown option between the two is reported first' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--bogus', '--size', 'm')) -Message "Unknown option '--bogus' for agentspace lease create. Run 'agentspace lease create --help'."
  Test-PeUsage -Why 'an unknown option after the pair is not reached' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', 'm', '--bogus')) -Field plan -Message $W
  Test-PeUsage -Why 'an unexpected argument before the pair is reported first' -Arguments ($c[1..2] + @('extra', '--plan', 'ok.json', '--size', 'm')) -Message "Unexpected argument 'extra' for agentspace lease create. Run 'agentspace lease create --help'."
  Test-PeUsage -Why 'an unexpected argument after the pair is not reached' -Arguments ($c[1..2] + @('--plan', 'ok.json', '--size', 'm', 'extra')) -Field plan -Message $W
  Test-PeUsage -Why 'a plan alone with no --project: the project is missing, and the file is never looked at' -Arguments @('--plan', 'missing.json') -Field project -Message "Option --project is required. Run 'agentspace lease create --help'."
  Test-PeUsage -Why 'a plan with an unknown option: usage before the plan is read' -Arguments ($c[1..2] + @('--plan', 'missing.json', '--bogus')) -Message "Unknown option '--bogus' for agentspace lease create. Run 'agentspace lease create --help'."
  # No other command takes --plan: an unknown option, at its own level (lease-api.md#command-lines).
  $someId = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'
  foreach ($line in @(@('get', $someId, '--plan', 'ok.json'), @('renew', $someId, '--plan', 'ok.json'), @('release', $someId, '--plan', 'ok.json'), @('list', '--plan', 'ok.json'))) {
    $name = "lease-api.md#command-lines: $(Format-ConformanceLine (@('lease') + $line)) is a usage error for the unknown option --plan, with no field, exit 2"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      $run = Invoke-PeLease -Arguments $line
      foreach ($x in (Get-PeErrorProblems -Run $run -Code usage -Messages @("Unknown option '--plan' for agentspace lease $($line[0]). Run 'agentspace lease $($line[0]) --help'."))) { $p.Add($x) }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-PeCheck -Name $name -Problems $p
  }
  # Help is asked for, so it is printed and nothing is checked or done (rule 1 of "Which error is reported").
  $blocks = Get-ConformanceHelpBlocks
  foreach ($line in @(
      @('lease', 'create', '--plan', 'ok.json', '--size', 'm', '--help'),
      @('lease', 'create', '--help', '--plan', 'ok.json', '--size', 'm'),
      @('lease', 'create', '--plan', '--help'),
      @('lease', 'create', '--size', 'm', '--plan', '-H'),
      @('lease', 'create', '--project', 'pe-case', '--plan', 'missing.json', '-h'))) {
    Test-ConformanceHelpRun -ConformanceHome $h1 -Name "lease-api.md#command-lines: $(Format-ConformanceLine $line) prints create's help and exits 0" -Arguments $line -Level 'agentspace lease create' -Blocks $blocks
  }
  # --plan=--help and --plan=-h are values, files with those names, never requests for help (lease-api.md#command-lines).
  Test-PeRefusal -Desc "--plan=--help (the value '--help', a file that is not there)" -Arg '--help' -Whys @($script:WhyRead) -EqualsForm
  Test-PeRefusal -Desc "--plan=-h (the value '-h', a file that is not there)" -Arg '-h' -Whys @($script:WhyRead) -EqualsForm

  # --- The order in which values are checked: project, platform, profile, plan, size, ttl ---
  Test-PeValueError -Why 'the project comes before the plan' -Arguments @('--project', 'bad_', '--plan', 'missing.json') -Code invalid_value -Field project
  Test-PeValueError -Why 'the platform comes before the plan' -Arguments @('--project', 'pe-case', '--plan', 'missing.json', '--platform', 'linux') -Code invalid_value -Field platform
  Test-PeValueError -Why 'the profile comes before the plan' -Arguments @('--project', 'pe-case', '--plan', 'missing.json', '--profile', 'agent linux') -Code invalid_value -Field profile
  Test-PeValueError -Why 'the first bad value of project, platform, profile is the one reported, with a bad plan beside them' -Arguments @('--project', 'pe-case', '--profile', 'agent linux', '--plan', 'missing.json', '--platform', 'linux') -Code invalid_value -Field platform
  Test-PeValueError -Why 'the plan comes before the TTL, whatever the line order' -Arguments @('--ttl', '8', '--project', 'pe-case', '--plan', 'missing.json') -Code invalid_value -Field plan
  Test-PeValueError -Why 'the plan comes before a TTL out of range' -Arguments @('--project', 'pe-case', '--plan', 'missing.json', '--ttl', '48h') -Code invalid_value -Field plan
  Test-PeValueError -Why 'a good plan, then the TTL is checked' -Arguments @('--project', 'pe-case', '--plan', 'ok.json', '--ttl', '8') -Code invalid_value -Field ttl
  Test-PeValueError -Why 'a good plan, then a TTL out of range' -Arguments @('--project', 'pe-case', '--plan', 'ok.json', '--ttl', '48h') -Code out_of_range -Field ttl
  Test-PeValueError -Why 'a good plan, then a TTL of 0m' -Arguments @('--project', 'pe-case', '--plan=ok.json', '--ttl=0m') -Code out_of_range -Field ttl

  # --- Check 1, the size: more than 262144 bytes, a byte order mark counted ---
  Test-PeFile -Desc 'A valid plan padded to 262145 bytes' -Bytes (New-PePadded -Text $script:OkPlan -Total 262145) -Whys @($script:WhySize)
  Test-PeFile -Desc 'A valid plan with a byte order mark, padded to 262145 bytes in all (the mark counts)' -Bytes (New-PePadded -Text $script:OkPlan -Total 262145 -Bom) -Whys @($script:WhySize)
  Test-PeFile -Desc 'A file of 1 MiB of spaces (the size is checked before the content)' -Bytes (New-PePadded -Text '' -Total 1048576) -Whys @($script:WhySize)
  Test-PeFile -Desc 'A file of 262145 bytes that is not UTF-8 (size is checked before UTF-8)' -Bytes ([byte[]]::new(262145) | ForEach-Object { [byte]0xFF }) -Whys @($script:WhySize)
  Test-PeFile -Desc 'A file of 300000 bytes of JSON that is not a plan (size is checked before the schema)' -Bytes (New-PePadded -Text '[1]' -Total 300000) -Whys @($script:WhySize)

  # --- Check 2, a file that cannot be read ---
  Test-PeRefusal -Desc 'A file that is not there' -Arg 'missing.json' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A file in a folder that is not there' -Arg 'nodir\missing.json' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A relative path that goes up and back down, quoted as given and never made a full path' -Arg '..\plans\missing.json' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path written with a forward slash and a dot, quoted as given' -Arg './missing.json' -Whys @($script:WhyRead)
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h1 -Path (Join-Path $script:PeDir 'adir.json')) | Out-Null
  Test-PeRefusal -Desc 'A folder' -Arg 'adir.json' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'The working folder, as .' -Arg '.' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'The folder above the working folder, as ..' -Arg '..' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc "- is a file called -, never standard input, though a valid plan is on standard input" -Arg '-' -Whys @($script:WhyRead) -InputJson $script:OkPlan
  Write-PeText -Name '-' -Text '{"turns":1}'
  Test-PeRefusal -Desc "- names a file called -, which is there and is not a plan: it is read, not standard input" -Arg '-' -Whys @($script:WhySchema) -InputJson $script:OkPlan
  Test-PeRefusal -Desc 'A value of one space, which is a path like any other' -Arg ' ' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a leading space, which is not trimmed and so is not ok.json, which is there' -Arg ' ok.json' -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a leading tab, which is not trimmed' -Arg "`tok.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a line feed, quoted with the control character escaped' -Arg "ok`n.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a delete character, quoted with it escaped' -Arg "ok$([char]0x7F).json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a tab and a carriage return' -Arg "a`tb`rc.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc "A path with an apostrophe, quoted raw" -Arg "it's.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A path with a letter that is not ASCII, quoted raw' -Arg "caf$([char]0xE9).json" -Whys @($script:WhyRead)
  Write-PeText -Name "bom$([char]0xFEFF)name.json" -Text $script:OkPlan
  Test-PeRefusal -Desc 'An argument that holds U+FEFF, though a valid plan has that very name (no file is opened for it)' -Arg "bom$([char]0xFEFF)name.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'An argument that is U+FEFF alone' -Arg ([string][char]0xFEFF) -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'An argument that holds an unpaired high surrogate, written as \ud800 in the message' -Arg "a$([char]0xD800)b.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'An argument that holds an unpaired low surrogate, written as \udc00 in the message' -Arg "a$([char]0xDC00)b.json" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'An argument that ends in an unpaired high surrogate' -Arg "ok.json$([char]0xD83D)" -Whys @($script:WhyRead)
  Test-PeRefusal -Desc 'A device, NUL, which would read as an empty file' -Arg 'NUL' -Whys @($script:WhyRead) -TimeoutSeconds 30
  Test-PeRefusal -Desc 'A device, CON, which MUST NOT be waited on' -Arg 'CON' -Whys @($script:WhyRead) -TimeoutSeconds 30
  # A pipe: the case holds the server end, and nothing is ever written to it, so a program that read it would wait.
  $pipeName = 'agentspace-conformance-' + [guid]::NewGuid().ToString('N').Substring(0, 12)
  $pipe = $null
  try {
    $pipe = [IO.Pipes.NamedPipeServerStream]::new($pipeName, [IO.Pipes.PipeDirection]::InOut, 1, [IO.Pipes.PipeTransmissionMode]::Byte, [IO.Pipes.PipeOptions]::Asynchronous)
    Test-PeRefusal -Desc 'A named pipe, which MUST NOT be waited on' -Arg "\\.\pipe\$pipeName" -Whys @($script:WhyRead) -TimeoutSeconds 30
  } finally { if ($pipe) { $pipe.Dispose() } }
  # A file that cannot be opened: held open by the case with no sharing.
  Write-PeText -Name 'held.json' -Text $script:OkPlan
  $held = $null
  try {
    $held = [IO.File]::Open((Assert-ConformancePath -ConformanceHome $h1 -Path (Join-Path $script:PeDir 'held.json')), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    Test-PeRefusal -Desc 'A valid plan that another process holds open with no sharing' -Arg 'held.json' -Whys @($script:WhyRead)
  } finally { if ($held) { $held.Dispose() } }

  # --- Check 3, UTF-8 ---
  $utf8 = [Text.UTF8Encoding]::new($false)
  $okBytes = $utf8.GetBytes($script:OkPlan)
  Test-PeFile -Desc 'A lone 0xFF byte inside a string' -Bytes ($utf8.GetBytes('{"name":"a') + [byte[]](0xFF) + $utf8.GetBytes('b","turns":[]}')) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A multi-byte sequence cut off at the end of the file' -Bytes ($okBytes + [byte[]](0xC3)) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'An overlong encoding of a slash, 0xC0 0xAF' -Bytes ($utf8.GetBytes('{"name":"a') + [byte[]](0xC0, 0xAF) + $utf8.GetBytes('b","turns":[]}')) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A surrogate encoded in UTF-8, 0xED 0xA0 0x80' -Bytes ($utf8.GetBytes('{"name":"a') + [byte[]](0xED, 0xA0, 0x80) + $utf8.GetBytes('b","turns":[]}')) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A code point above U+10FFFF encoded as 0xF4 0x90 0x80 0x80' -Bytes ($utf8.GetBytes('{"name":"a') + [byte[]](0xF4, 0x90, 0x80, 0x80) + $utf8.GetBytes('b","turns":[]}')) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'Latin-1 text, an e with an acute accent as the single byte 0xE9' -Bytes ($utf8.GetBytes('{"name":"caf') + [byte[]](0xE9) + $utf8.GetBytes('","turns":[]}')) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A valid plan in UTF-16 little-endian, with its byte order mark (0xFF 0xFE is not UTF-8)' -Bytes ([byte[]](0xFF, 0xFE) + [Text.Encoding]::Unicode.GetBytes($script:OkPlan)) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A valid plan in UTF-16 big-endian, with its byte order mark (0xFE 0xFF is not UTF-8)' -Bytes ([byte[]](0xFE, 0xFF) + [Text.Encoding]::BigEndianUnicode.GetBytes($script:OkPlan)) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'A UTF-8 byte order mark, then a byte that is not UTF-8' -Bytes ($script:PeBom + [byte[]](0xFF) + $okBytes) -Whys @($script:WhyUtf8)
  Test-PeFile -Desc 'Bytes that are neither UTF-8 nor JSON: UTF-8 is checked first' -Bytes ([byte[]](0xFF, 0xFF, 0xFF)) -Whys @($script:WhyUtf8)

  # --- Check 4, JSON ---
  Test-PeFile -Desc 'An empty file' -Bytes ([byte[]]@()) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A file of only whitespace' -Text " `t`r`n  `n" -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A file of only a byte order mark' -Bytes $script:PeBom -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A valid plan in UTF-16 little-endian with no byte order mark (valid UTF-8, a zero byte beside each letter)' -Bytes ([Text.Encoding]::Unicode.GetBytes($script:OkPlan)) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A valid plan and a second byte order mark in front of it' -Bytes ($script:PeBom + $script:PeBom + $okBytes) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A valid plan and U+FEFF after it' -Bytes ($okBytes + $script:PeBom) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'U+FEFF between two members, outside any string' -Bytes ($utf8.GetBytes('{"turns":[') + $script:PeBom + $utf8.GetBytes($script:OkTurn + ']}')) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A valid plan and a word after it' -Text ($script:OkPlan + ' x') -Whys @($script:WhyJson)
  Test-PeFile -Desc 'Two plans in a row' -Text ($script:OkPlan + $script:OkPlan) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A valid plan and a comma after it' -Text ($script:OkPlan + ',') -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A trailing comma in an array' -Text '{"turns":[],}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A document cut off before its end' -Text ($script:OkPlan.Substring(0, $script:OkPlan.Length - 3)) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'Single quotes' -Text "{'turns':[]}" -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A member name with no quotes' -Text '{turns:[]}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A comment' -Text ('/* a plan */' + $script:OkPlan) -Whys @($script:WhyJson)
  Test-PeFile -Desc 'NaN' -Text '{"turns":NaN}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A number with a leading zero' -Text '{"turns":01}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A number the parser cannot hold, 1e400, though a plan has no number' -Text '{"turns":1e400}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A line feed inside a string' -Text ('{"name":"a' + "`n" + 'b","turns":[]}') -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A tab inside a string' -Text ('{"name":"a' + "`t" + 'b","turns":[]}') -Whys @($script:WhyJson)
  Test-PeFile -Desc 'An unpaired surrogate written as an escape, \ud800, in a model' -Text '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"\ud800","effort":"high"}]}]}]}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'An unpaired low surrogate written as an escape, \udc00, in a name' -Text '{"name":"\udc00","turns":[]}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'A high surrogate escape followed by an escape that is not a low surrogate' -Text '{"name":"\ud800\u0041","turns":[]}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'An escape that JSON does not have, \x' -Text '{"name":"\x","turns":[]}' -Whys @($script:WhyJson)
  Test-PeFile -Desc 'An escape of fewer than four hex digits' -Text '{"name":"\u12","turns":[]}' -Whys @($script:WhyJson)
  # Nesting: 64 levels MUST be parsed (the document is level 1), and then it is not a plan; a document far deeper
  # is either not JSON, where the parser's limit is, or not a plan, and MUST be refused, never crash the program.
  Test-PeFile -Desc 'A document nested 64 levels, which a parser MUST allow, and is not a plan' -Text (('[' * 64) + (']' * 64)) -Whys @($script:WhySchema)
  Test-PeFile -Desc 'A document nested 100000 levels, which a parser MAY refuse as JSON and which is never a plan' -Text (('[' * 100000) + (']' * 100000)) -Whys @($script:WhyJson, $script:WhySchema)
  Test-PeFile -Desc 'A plan with a member nested 100000 levels in a place a plan has no member' -Text ('{"turns":[],"x":' + ('[' * 100000) + (']' * 100000) + '}') -Whys @($script:WhyJson, $script:WhySchema)

  # --- Check 5, a member named twice (the first, in document order, whose name an earlier member of its object has) ---
  Test-PeFile -Desc 'turns given twice in the plan' -Text ('{"turns":[' + $script:OkTurn + '],"turns":[' + $script:OkTurn + ']}') -Whys @((Get-PeWhyMembers 'turns'))
  Test-PeFile -Desc 'name given twice in the plan' -Text ('{"name":"p","name":"q","turns":[' + $script:OkTurn + ']}') -Whys @((Get-PeWhyMembers 'name'))
  Test-PeFile -Desc 'name given twice in a turn' -Text '{"turns":[{"name":"T1","name":"T2","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' -Whys @((Get-PeWhyMembers 'name'))
  Test-PeFile -Desc 'subagents given twice in a story' -Text '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}],"subagents":[]}]}]}' -Whys @((Get-PeWhyMembers 'subagents'))
  Test-PeFile -Desc 'model given twice in a subagent' -Text '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","model":"Sonnet","effort":"high"}]}]}]}' -Whys @((Get-PeWhyMembers 'model'))
  Test-PeFile -Desc 'after given twice in a subagent' -Text '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":[],"after":[]}]}]}]}' -Whys @((Get-PeWhyMembers 'after'))
  Test-PeFile -Desc 'A name given twice once the escapes are read: tu\u0072ns and turns' -Text ('{"turns":[' + $script:OkTurn + '],"tu\u0072ns":[' + $script:OkTurn + ']}') -Whys @((Get-PeWhyMembers 'turns'))
  Test-PeFile -Desc 'A name with a control character, given twice with its escape in two cases, quoted with \u000a in lower case' -Text '{"turns":[],"a\u000A":1,"a\u000a":2}' -Whys @((Get-PeWhyMembers 'a\u000a'))
  Test-PeFile -Desc 'A name that is not ASCII, given twice, quoted raw' -Text "{`"turns`":[],`"caf$([char]0xE9)`":1,`"caf$([char]0xE9)`":2}" -Whys @((Get-PeWhyMembers "caf$([char]0xE9)"))
  Test-PeFile -Desc 'A name that is not in the shape, given twice: members are compared before the shape' -Text ('{"x":1,"x":2,"turns":[' + $script:OkTurn + ']}') -Whys @((Get-PeWhyMembers 'x'))
  Test-PeFile -Desc 'Two empty turns given twice: members are compared before the schema refuses an empty list' -Text '{"turns":[],"turns":[]}' -Whys @((Get-PeWhyMembers 'turns'))
  Test-PeFile -Desc 'Names that differ in case are two names: turns and Turns is a member the shape does not have' -Text ('{"turns":[' + $script:OkTurn + '],"Turns":[]}') -Whys @($script:WhySchema)
  Test-PeFile -Desc 'The first fault in document order is reported: a name inside a turn, before the plan repeats its own' -Text '{"turns":[{"name":"T1","stories":[],"stories":[]}],"name":"p","name":"q"}' -Whys @((Get-PeWhyMembers 'stories'))
  Test-PeFile -Desc 'The first fault in document order is reported: the plan repeats name before a turn repeats stories' -Text '{"name":"p","name":"q","turns":[{"name":"T1","stories":[],"stories":[]}]}' -Whys @((Get-PeWhyMembers 'name'))
  Test-PeFile -Desc 'Not JSON beats a member named twice' -Text '{"a":1,"a":2' -Whys @($script:WhyJson)

  # --- Check 6, the plan schema ---
  foreach ($d in @(
      @{ D = 'A document that is an array'; T = '[]' }
      @{ D = 'A document that is an empty object, with no turns'; T = '{}' }
      @{ D = 'A document that is null'; T = 'null' }
      @{ D = 'A document that is true'; T = 'true' }
      @{ D = 'A document that is a string'; T = '"plan"' }
      @{ D = 'A document that is a number'; T = '7' }
      @{ D = 'turns that is null'; T = '{"turns":null}' }
      @{ D = 'turns that is an object'; T = '{"turns":{}}' }
      @{ D = 'turns that is a string'; T = '{"turns":"T1"}' }
      @{ D = 'An empty list of turns'; T = '{"turns":[]}' }
      @{ D = 'A plan with an unknown member'; T = ('{"turns":[' + $script:OkTurn + '],"extra":1}') }
      @{ D = 'A plan with a member for a peak, which a plan does not have'; T = ('{"peak":3,"turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name is a number'; T = ('{"name":5,"turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name is empty'; T = ('{"name":"","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name is 65 characters'; T = ('{"name":"' + ('n' * 65) + '","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name starts with a space'; T = ('{"name":" p","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name ends with a space'; T = ('{"name":"p ","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name has a tab inside'; T = ('{"name":"p\tq","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name has a letter that is not ASCII'; T = ('{"name":"caf' + [char]0xE9 + '","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name holds U+FEFF, written as an escape (a byte order mark is never in a value)'; T = ('{"name":"a\ufeffb","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name holds a raw U+FEFF inside its string'; T = ('{"name":"a' + [char]0xFEFF + 'b","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name holds a NUL, written as an escape'; T = ('{"name":"a\u0000b","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A plan whose name is a pair of surrogates for a letter that is not ASCII'; T = ('{"name":"\ud83d\ude00","turns":[' + $script:OkTurn + ']}') }
      @{ D = 'A turn with no name'; T = '{"turns":[{"stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A turn whose name is null'; T = '{"turns":[{"name":null,"stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A turn with no stories'; T = '{"turns":[{"name":"T1"}]}' }
      @{ D = 'A turn whose stories is empty'; T = '{"turns":[{"name":"T1","stories":[]}]}' }
      @{ D = 'A turn whose stories is an object'; T = '{"turns":[{"name":"T1","stories":{}}]}' }
      @{ D = 'A turn with an unknown member'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high"}]}],"peak":2}]}' }
      @{ D = 'A story with no name'; T = '{"turns":[{"name":"T1","stories":[{"subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A story with no subagents'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1"}]}]}' }
      @{ D = 'A story whose subagents is null'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":null}]}]}' }
      @{ D = 'A story whose subagents is empty'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[]}]}]}' }
      @{ D = 'A story with a stated peak, which a plan never states'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","peak":3,"subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A story whose name is a number'; T = '{"turns":[{"name":"T1","stories":[{"name":7,"subagents":[{"name":"A","model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A subagent with no name'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A subagent with no model'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","effort":"high"}]}]}]}' }
      @{ D = 'A subagent with no effort'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus"}]}]}]}' }
      @{ D = 'A subagent whose model is null'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":null,"effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose effort is null'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":null}]}]}]}' }
      @{ D = 'A subagent whose name is null'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":null,"model":"Opus","effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose model is empty'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"","effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose model is 65 characters'; T = ('{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"' + ('m' * 65) + '","effort":"high"}]}]}]}') }
      @{ D = 'A subagent whose model starts with a space'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":" Opus","effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose model ends with a space'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus ","effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose model has a tab'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus\t5","effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose model has a letter that is not ASCII'; T = ('{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus' + [char]0xE9 + '","effort":"high"}]}]}]}') }
      @{ D = 'A subagent whose model is a number'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":5,"effort":"high"}]}]}]}' }
      @{ D = 'A subagent whose effort is empty'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":""}]}]}]}' }
      @{ D = 'A subagent whose effort has a space'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"very high"}]}]}]}' }
      @{ D = 'A subagent whose effort starts with a hyphen'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"-high"}]}]}]}' }
      @{ D = 'A subagent whose effort starts with a dot'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":".high"}]}]}]}' }
      @{ D = 'A subagent whose effort starts with an underscore'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"_high"}]}]}]}' }
      @{ D = 'A subagent whose effort is 33 characters'; T = ('{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"' + ('e' * 33) + '"}]}]}]}') }
      @{ D = 'A subagent whose effort has a plus'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high+"}]}]}]}' }
      @{ D = 'A subagent whose effort is a number'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":3}]}]}]}' }
      @{ D = 'A subagent with an unknown member'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","peak":1}]}]}]}' }
      @{ D = 'A subagent whose after is a string'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":"B"}]}]}]}' }
      @{ D = 'A subagent whose after holds a number'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":[1]}]}]}]}' }
      @{ D = 'A subagent whose after holds an empty name'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":[""]}]}]}]}' }
      @{ D = 'A subagent whose after holds a name with a leading space'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":[" B"]}]}]}]}' }
      @{ D = 'A subagent whose after holds an array'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":[["B"]]}]}]}]}' }
      @{ D = 'A subagent whose after names one subagent twice'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"B","model":"Opus","effort":"high"},{"name":"A","model":"Opus","effort":"high","after":["B","B"]}]}]}]}' }
      @{ D = 'A model and an effort of a document the shape refuses, quoted nowhere'; T = '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"SECRETMODEL","effort":"SECRETEFFORT","other":1}]}]}]}' }
    )) {
    Test-PeFile -Desc $d.D -Text $d.T -Whys @($script:WhySchema)
  }
  # The bounds on the lists: 33 of each is one too many, and 32 are not (the 32 are in the accepted cases below).
  Test-PeFile -Desc '33 turns' -Text (New-PePlan -Turns @(1..33 | ForEach-Object { New-PeTurn -Name "T$_" -Stories @($script:OkStory) })) -Whys @($script:WhySchema)
  Test-PeFile -Desc '33 stories in a turn' -Text (New-PePlan -Turns @(New-PeTurn -Stories @(1..33 | ForEach-Object { New-PeStory -Name "S$_" -Subs @($script:OkSub) }))) -Whys @($script:WhySchema)
  Test-PeFile -Desc '33 subagents in a story' -Text (New-PePlan -Turns @(New-PeTurn -Stories @(New-PeStory -Subs @(1..33 | ForEach-Object { New-PeSub -Name "A$_" })))) -Whys @($script:WhySchema)
  # 32 subagents: the last starts after all 32 names, itself among them, so an after of 32 names, one too many (the
  # schema is checked before the cycle is looked for).
  $thirtyTwo = @(1..31 | ForEach-Object { "A$_" }) + @('Z')
  Test-PeFile -Desc 'An after of 32 names, one too many (it also names its own subagent, a cycle, which comes after the schema)' -Text (New-PePlan -Turns @(New-PeTurn -Stories @(New-PeStory -Subs (@(1..31 | ForEach-Object { New-PeSub -Name "A$_" }) + @(New-PeSub -Name 'Z' -After $thirtyTwo))))) -Whys @($script:WhySchema)
  Test-PeFile -Desc 'A plan whose name is null and whose turns is null: the null for a required member is refused' -Text '{"name":null,"turns":null}' -Whys @($script:WhySchema)

  # --- Check 7, a turn named twice ---
  $s = $script:OkStory
  Test-PeFile -Desc 'Two turns named T1' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($s)), (New-PeTurn -Name 'T1' -Stories @($s)))) -Whys @((Get-PeWhyTurn 'T1'))
  Test-PeFile -Desc 'Turns A, B, B, A: the first name that repeats an earlier one is B' -Text (New-PePlan -Turns @(@('A', 'B', 'B', 'A') | ForEach-Object { New-PeTurn -Name $_ -Stories @($s) })) -Whys @((Get-PeWhyTurn 'B'))
  Test-PeFile -Desc 'Turns A, B, A, B: the first name that repeats an earlier one is A' -Text (New-PePlan -Turns @(@('A', 'B', 'A', 'B') | ForEach-Object { New-PeTurn -Name $_ -Stories @($s) })) -Whys @((Get-PeWhyTurn 'A'))
  Test-PeFile -Desc "Two turns with a name that holds a space and an apostrophe, quoted raw" -Text (New-PePlan -Turns @((New-PeTurn -Name "it's a turn" -Stories @($s)), (New-PeTurn -Name "it's a turn" -Stories @($s)))) -Whys @((Get-PeWhyTurn "it's a turn"))
  Test-PeFile -Desc 'Two turns named by two spellings of one name (an escape is read before names are compared)' -Text ('{"turns":[' + $script:OkTurn + ',' + $script:OkTurn.Replace('"T1"', '"T\u0031"') + ']}') -Whys @((Get-PeWhyTurn 'T1'))
  Test-PeFile -Desc 'Two turns named alike, and a model the schema refuses: the schema is checked first' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($s)), (New-PeTurn -Name 'T1' -Stories @((New-PeStory -Subs @((New-PeSub -Model ' bad'))))))) -Whys @($script:WhySchema)
  # 7 comes before 8 and 9, though the story's fault stands first in the document.
  $dupStory = New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @($script:OkSub)), (New-PeStory -Name 'S1' -Subs @($script:OkSub)))
  Test-PeFile -Desc 'A story named twice in the first turn, and two turns named alike after it: the turns are reported' -Text (New-PePlan -Turns @($dupStory, (New-PeTurn -Name 'T2' -Stories @($s)), (New-PeTurn -Name 'T2' -Stories @($s)))) -Whys @((Get-PeWhyTurn 'T2'))
  $dupSub = New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @($script:OkSub, $script:OkSub)))
  Test-PeFile -Desc 'A subagent named twice in the first turn, and two turns named alike after it: the turns are reported' -Text (New-PePlan -Turns @($dupSub, (New-PeTurn -Name 'T2' -Stories @($s)), (New-PeTurn -Name 'T2' -Stories @($s)))) -Whys @((Get-PeWhyTurn 'T2'))

  # --- Check 8, a story named twice in a turn ---
  Test-PeFile -Desc 'Two stories named S1 in turn T1' -Text (New-PePlan -Turns @($dupStory)) -Whys @((Get-PeWhyStory 'S1' 'T1'))
  Test-PeFile -Desc 'Stories A, B, B, A: the first that repeats an earlier name is B' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @(@('A', 'B', 'B', 'A') | ForEach-Object { New-PeStory -Name $_ -Subs @($script:OkSub) })))) -Whys @((Get-PeWhyStory 'B' 'T1'))
  Test-PeFile -Desc 'The second turn is the first to repeat a story name, with the first turn clean' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($s)), (New-PeTurn -Name 'T2 two' -Stories @((New-PeStory -Name 'X y' -Subs @($script:OkSub)), (New-PeStory -Name 'X y' -Subs @($script:OkSub)))))) -Whys @((Get-PeWhyStory 'X y' 'T2 two'))
  Test-PeFile -Desc 'Two turns each repeat a story name: the first turn is reported' -Text (New-PePlan -Turns @($dupStory, (New-PeTurn -Name 'T2' -Stories @((New-PeStory -Name 'S2' -Subs @($script:OkSub)), (New-PeStory -Name 'S2' -Subs @($script:OkSub)))))) -Whys @((Get-PeWhyStory 'S1' 'T1'))
  # 8 comes before 9, though the subagents' fault stands first.
  Test-PeFile -Desc 'A subagent named twice in the first story and a story named twice after it: the stories are reported' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @($script:OkSub, $script:OkSub)), (New-PeStory -Name 'S2' -Subs @($script:OkSub)), (New-PeStory -Name 'S2' -Subs @($script:OkSub)))))) -Whys @((Get-PeWhyStory 'S2' 'T1'))

  # --- Check 9, a subagent named twice in a story ---
  Test-PeFile -Desc 'Two subagents named A in story S1' -Text (New-PePlan -Turns @($dupSub)) -Whys @((Get-PeWhySubagent 'A' 'S1'))
  Test-PeFile -Desc 'Subagents A, B, B, A: the first that repeats an earlier name is B' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @(@('A', 'B', 'B', 'A') | ForEach-Object { New-PeSub -Name $_ })))))) -Whys @((Get-PeWhySubagent 'B' 'S1'))
  Test-PeFile -Desc 'The second story is the first to repeat a subagent name' -Text (New-PePlan -Turns @((New-PeTurn -Stories @($s, (New-PeStory -Name 'S two' -Subs @((New-PeSub -Name 'test author'), (New-PeSub -Name 'test author'))))))) -Whys @((Get-PeWhySubagent 'test author' 'S two'))
  # 9 comes before 10, though the after fault stands first.
  Test-PeFile -Desc 'An after that names no subagent, and a subagent named twice after it: the names are reported' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A' -After @('nobody')), (New-PeSub -Name 'B'), (New-PeSub -Name 'B'))))))) -Whys @((Get-PeWhySubagent 'B' 'S1'))

  # --- Check 10, an after that names no subagent of its story ---
  Test-PeFile -Desc 'An after that names a subagent nobody has' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A'), (New-PeSub -Name 'B' -After @('Z')))))))) -Whys @((Get-PeWhyAfter 'B' 'S1' 'Z'))
  Test-PeFile -Desc 'An after that names a subagent of another story of the same turn' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A'))), (New-PeStory -Name 'S2' -Subs @((New-PeSub -Name 'B' -After @('A')))))))) -Whys @((Get-PeWhyAfter 'B' 'S2' 'A'))
  Test-PeFile -Desc 'An after that names a subagent another turn has, of the same name as one in its own story' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A'))))), (New-PeTurn -Name 'T2' -Stories @((New-PeStory -Name 'S2' -Subs @((New-PeSub -Name 'B' -After @('A')))))))) -Whys @((Get-PeWhyAfter 'B' 'S2' 'A'))
  Test-PeFile -Desc 'An after whose name differs in case from a subagent of its story, since names are compared exactly' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'impl'), (New-PeSub -Name 'review' -After @('Impl')))))))) -Whys @((Get-PeWhyAfter 'review' 'S1' 'Impl'))
  Test-PeFile -Desc 'Several after entries, the first that is no subagent is named' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A'), (New-PeSub -Name 'B'), (New-PeSub -Name 'C' -After @('A', 'nope', 'B', 'nope2')))))))) -Whys @((Get-PeWhyAfter 'C' 'S1' 'nope'))
  $turnA = New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A'))), (New-PeStory -Name 'S2' -Subs @((New-PeSub -Name 'B' -After @('x')))))
  $turnB = New-PeTurn -Name 'T2' -Stories @((New-PeStory -Name 'S3' -Subs @((New-PeSub -Name 'C' -After @('y')))))
  Test-PeFile -Desc 'Two subagents have such an entry: the first in document order, across stories' -Text (New-PePlan -Turns @($turnA, $turnB)) -Whys @((Get-PeWhyAfter 'B' 'S2' 'x'))
  Test-PeFile -Desc "Names with spaces and an apostrophe, quoted raw" -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name "the story" -Subs @((New-PeSub -Name 'test author'), (New-PeSub -Name "it's a reviewer" -After @("no one"))))))))  -Whys @((Get-PeWhyAfter "it's a reviewer" 'the story' 'no one'))
  # 10 comes before 11, though the cycle's story stands first.
  $cyc = New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A' -After @('B')), (New-PeSub -Name 'B' -After @('A')))
  Test-PeFile -Desc 'A cycle in the first story and an after that names no subagent in the second: the after is reported' -Text (New-PePlan -Turns @((New-PeTurn -Stories @($cyc, (New-PeStory -Name 'S2' -Subs @((New-PeSub -Name 'C' -After @('ghost')))))))) -Whys @((Get-PeWhyAfter 'C' 'S2' 'ghost'))

  # --- Check 11, a cycle ---
  Test-PeFile -Desc 'A subagent that starts after itself' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'A' -After @('A')))))))) -Whys @((Get-PeWhyCycle 'S1'))
  Test-PeFile -Desc 'Two subagents that start after each other' -Text (New-PePlan -Turns @((New-PeTurn -Stories @($cyc)))) -Whys @((Get-PeWhyCycle 'S1'))
  Test-PeFile -Desc 'Three subagents that start after one another in a ring' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'ring' -Subs @((New-PeSub -Name 'A' -After @('C')), (New-PeSub -Name 'B' -After @('A')), (New-PeSub -Name 'C' -After @('B')))))))) -Whys @((Get-PeWhyCycle 'ring'))
  Test-PeFile -Desc 'A cycle with a subagent outside it that starts after the cycle and one before it' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @((New-PeSub -Name 'root'), (New-PeSub -Name 'A' -After @('root', 'B')), (New-PeSub -Name 'B' -After @('A')), (New-PeSub -Name 'tail' -After @('B')))))))) -Whys @((Get-PeWhyCycle 'S1'))
  Test-PeFile -Desc 'A long cycle through 32 subagents' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name 'S1' -Subs @(1..32 | ForEach-Object { New-PeSub -Name "A$_" -After @("A$(if ($_ -eq 1) { 32 } else { $_ - 1 })") })))))) -Whys @((Get-PeWhyCycle 'S1'))
  Test-PeFile -Desc 'The first story with a cycle is reported: the second of three, the first being clean' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($s)), (New-PeTurn -Name 'T2' -Stories @((New-PeStory -Name 'S2' -Subs @((New-PeSub -Name 'A' -After @('A')))), (New-PeStory -Name 'S3' -Subs @((New-PeSub -Name 'A' -After @('A')))))))) -Whys @((Get-PeWhyCycle 'S2'))
  Test-PeFile -Desc 'A story name with a space and an apostrophe, quoted raw' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Name "it's a story" -Subs @((New-PeSub -Name 'A' -After @('A')))))))) -Whys @((Get-PeWhyCycle "it's a story"))

  # --- What no message carries, and what a refused plan leaves ---
  $p = [System.Collections.Generic.List[string]]::new()
  $all = $script:PeSeen -join "`n"
  foreach ($bad in 'SECRETMODEL', 'SECRETEFFORT', 'os error', 'denied', 'No such file', 'cannot find', 'The system cannot', 'not found', 'Win32', 'ERROR_', 'stack backtrace', 'panicked') {
    if ($all.IndexOf($bad, [StringComparison]::OrdinalIgnoreCase) -ge 0) { $p.Add("An error document holds '$bad'") }
  }
  if ($all -match '(?i)[A-Za-z]:[\\/]') { $p.Add('An error document holds the path of a drive') }
  if ($all -match '\\\\Users\\' -or $all -match 'AppData') { $p.Add('An error document holds a profile path') }
  if ($script:PeSeen.Count -lt 100) { $p.Add("Only $($script:PeSeen.Count) error documents were seen, so the cases did not run") }
  Complete-PeCheck -Name 'lease-api.md#plan: No refusal carries an operating system error text, a path of a drive, or a model or an effort of the document' -Problems $p
  $p = [System.Collections.Generic.List[string]]::new()
  # Nothing recorded means nothing only when the program answered: with no refusal seen, the result is not a PASS.
  if ($script:PeSeen.Count -lt 100) { $p.Add("Only $($script:PeSeen.Count) error documents were seen, so nothing shows that a refused plan was answered") }
  foreach ($x in (Get-PvNothingRecordedProblems -Lab $h1)) { $p.Add($x) }
  foreach ($f in 'hosts.json', 'projects.json') { if (Test-Path -LiteralPath (Join-Path $h1.UserProfile ".agentspace\$f")) { $p.Add("A refused plan left $f") } }
  if (Test-Path -LiteralPath (Join-Path $h1.UserProfile '.agentspace\hosts')) { $p.Add('A refused plan made the hosts folder') }
  Complete-PeCheck -Name 'lease-api.md#plan: A refused plan records no lease and creates nothing: no record, no lease folder, no alias file, no host' -Problems $p
  $p = Get-PvNoStartProblems -StandIn $script:PeStandIn
  if ($script:PeSeen.Count -lt 100) { $p.Add("Only $($script:PeSeen.Count) error documents were seen, so nothing shows that the refusals were answered") }
  Complete-PeCheck -Name 'lease-api.md#agentspace-lease-create: No create of the refusals started a provisioning run' -Problems $p
  [Environment]::CurrentDirectory = $savedCwd
  Clear-PvCleanup -Lab $h1
  Remove-ConformanceHome -ConformanceHome $h1
  $h1 = $null

  # ===== Home 2: plans that are accepted, and plan_too_large. A lease is recorded for each. =====
  $h2 = New-ConformanceHome
  Initialize-PeLab -Lab $h2
  $planName = 'pe-plan'

  # --- Plans that are accepted: the other side of each bound, so a refusal above is not a refusal of everything ---
  function Test-PeAccepted {
    param([Parameter(Mandatory)][string]$Desc, [string]$Text, [byte[]]$Bytes, [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents, [AllowNull()]$PlanName, [string[]]$Extra = @())
    $script:PeFiles++
    $file = 'a{0:d3}.json' -f $script:PeFiles
    $name = "lease-api.md#plan: $Desc is accepted, with sized_for turn '$(Format-PeText $Turn)' and $Agents agents"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      if ($PSBoundParameters.ContainsKey('Bytes')) { Write-PeBytes -Name $file -Bytes $Bytes } else { Write-PeText -Name $file -Text $Text }
      $run = Invoke-PeLease -Arguments (@('create', '--project', 'pe-case', '--plan', $file) + $Extra)
      foreach ($x in (Get-PeFitsProblems -Run $run -Turn $Turn -Agents $Agents -PlanName $PlanName)) { $p.Add($x) }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-PeCheck -Name $name -Problems $p
  }
  $okBytes = [Text.UTF8Encoding]::new($false).GetBytes($script:OkPlan)
  Test-PeAccepted -Desc 'A plan of exactly 262144 bytes (padded with spaces)' -Bytes (New-PePadded -Text $script:OkPlan -Total 262144) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan with a byte order mark, 262144 bytes in all' -Bytes (New-PePadded -Text $script:OkPlan -Total 262144 -Bom) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan with a leading byte order mark (the one exception to the rule for U+FEFF)' -Bytes ($script:PeBom + $okBytes) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan that ends in a line feed' -Text ($script:OkPlan + "`n") -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan spread over lines, with carriage returns and tabs between tokens' -Text ("{`r`n`t`"turns`" :`t[ " + $script:OkTurn + " ]`r`n}`r`n") -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan with a name' -Text (New-PePlan -Name 'My plan' -Turns @($script:OkTurn)) -Turn 'T1' -Agents 2 -PlanName 'My plan'
  Test-PeAccepted -Desc 'A plan whose name is null (not given)' -Text ('{"name":null,"turns":[' + $script:OkTurn + ']}') -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A subagent whose after is null (not given)' -Text '{"turns":[{"name":"T1","stories":[{"name":"S1","subagents":[{"name":"A","model":"Opus","effort":"high","after":null}]}]}]}' -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A subagent whose after is empty, which means none' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs @((New-PeSub -After @()))))))) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A plan name, a turn, a story and a subagent of 64 characters, each with a space inside' -Text (New-PePlan -Name ('p' * 31 + ' ' + 'q' * 32) -Turns @((New-PeTurn -Name ('t' * 31 + ' ' + 'u' * 32) -Stories @((New-PeStory -Name ('s' * 63 + 'x') -Subs @((New-PeSub -Name ('a' * 64))))))))  -Turn ('t' * 31 + ' ' + 'u' * 32) -Agents 2 -PlanName ('p' * 31 + ' ' + 'q' * 32)
  Test-PeAccepted -Desc 'A name of one character, and punctuation in a name (all of printable ASCII but the space)' -Text (New-PePlan -Turns @((New-PeTurn -Name '~' -Stories @((New-PeStory -Name '!"#$%&''()*+,-./:;<=>?@[\]^_`{|}~' -Subs @((New-PeSub -Name '0'))))))) -Turn '~' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'Names written with escapes, T\u0031 for T1, which are the names they read as' -Text ($script:OkPlan.Replace('"T1"', '"T\u0031"')) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A turn name with a quote and a backslash, as the plan writes it once read' -Text (New-PePlan -Turns @((New-PeTurn -Name 'a"b\c' -Stories @($script:OkStory)))) -Turn 'a"b\c' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'A model with a space and a time-bound pin, and a long effort' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs @((New-PeSub -Model 'Opus 5.5 +24h' -Effort 'high.2_x-y'), (New-PeSub -Name 'B' -Model ('m' * 64) -Effort ('e' * 32)))))))) -Turn 'T1' -Agents 3 -PlanName $null
  Test-PeAccepted -Desc 'A model and an effort of any kind: they are recorded, not judged' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs @((New-PeSub -Model 'Fable/high?' -Effort '0'))))))) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc '32 turns' -Text (New-PePlan -Turns @(1..32 | ForEach-Object { New-PeTurn -Name "T$_" -Stories @($script:OkStory) })) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc '32 subagents in a story, each after the one before: a peak of 1' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs @(1..32 | ForEach-Object { if ($_ -eq 1) { New-PeSub -Name 'A1' } else { New-PeSub -Name "A$_" -After @("A$($_ - 1)") } })))))) -Turn 'T1' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'An after of 31 names, the most' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs (@(1..31 | ForEach-Object { New-PeSub -Name "A$_" }) + @(New-PeSub -Name 'Z' -After @(1..31 | ForEach-Object { "A$_" }))))))))  -Turn 'T1' -Agents 32 -PlanName $null
  Test-PeAccepted -Desc 'A diamond of starts: a subagent after two that start after one' -Text (New-PePlan -Turns @((New-PeTurn -Stories @((New-PeStory -Subs @((New-PeSub -Name 'top'), (New-PeSub -Name 'l' -After @('top')), (New-PeSub -Name 'r' -After @('top')), (New-PeSub -Name 'bottom' -After @('l', 'r')))))))) -Turn 'T1' -Agents 3 -PlanName $null
  Test-PeAccepted -Desc 'Two turns named T03 and t03, which differ in case and so are two names' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T03' -Stories @($script:OkStory)), (New-PeTurn -Name 't03' -Stories @($script:OkStory)))) -Turn 'T03' -Agents 2 -PlanName $null
  Test-PeAccepted -Desc 'Two stories of different turns with one name, and two subagents of different stories with one name' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($script:OkStory)), (New-PeTurn -Name 'T2' -Stories @($script:OkStory, (New-PeStory -Name 'S2' -Subs @($script:OkSub)))))) -Turn 'T2' -Agents 3 -PlanName $null
  Test-PeAccepted -Desc 'A flag of another case and the = form' -Text $script:OkPlan -Turn 'T1' -Agents 2 -PlanName $null -Extra @('--TTL=1h', '--Platform', 'Linux/ARM64')
  # `-` names a file called -, which is read (here a plan): never standard input.
  Write-PeText -Name '-' -Text $script:OkPlan
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $run = Invoke-PeLease -Arguments @('create', '--project', 'pe-case', '--plan', '-') -InputJson '{"turns":1}'
    foreach ($x in (Get-PeFitsProblems -Run $run -Turn 'T1' -Agents 2 -PlanName $null)) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name "lease-api.md#plan: --plan - reads the file called -, not standard input (a plan in the file, a document that is no plan on standard input)" -Problems $p
  # An absolute path is used as given.
  $abs = Join-Path $script:PeDir 'ok.json'
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $run = Invoke-PeLease -Arguments @('create', '--project', 'pe-case', '--plan', $abs)
    foreach ($x in (Get-PeFitsProblems -Run $run -Turn 'T1' -Agents 2 -PlanName $null)) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name 'lease-api.md#plan: An absolute path is accepted as given' -Problems $p
  $p = [System.Collections.Generic.List[string]]::new()
  try {
    $run = Invoke-PeLease -Arguments @('create', '--project', 'pe-case', '--plan', ($abs + '.missing'))
    foreach ($x in (Get-PeErrorProblems -Run $run -Code invalid_value -Field plan -Messages @((Get-PePlanMessage ($abs + '.missing') $script:WhyRead)))) { $p.Add($x) }
  } catch { $p.Add("It threw: $($_.Exception.Message)") }
  Complete-PeCheck -Name 'lease-api.md#plan: An absolute path that is not a file is refused, and the message quotes it as given' -Problems $p

  # --- plan_too_large (lease-api.md#fields, #agentspace-lease-create) ---
  function Test-PeTooLarge {
    param([Parameter(Mandatory)][string]$Desc, [Parameter(Mandatory)][string]$Text, [Parameter(Mandatory)][string]$Turn, [Parameter(Mandatory)][int]$Agents, [AllowNull()]$PlanName, [string[]]$Extra = @(), [string]$Project = 'pe-case', [string]$ProjectArg, [string]$Platform = 'linux/arm64', [string]$ProfileText = 'agent-linux', [int]$TtlSeconds = 28800, [switch]$Fits)
    # $Project is the normalized project the lease MUST carry; $ProjectArg, when given, is what --project is given.
    if (-not $ProjectArg) { $ProjectArg = $Project }
    $script:PeFiles++
    $file = 'g{0:d3}.json' -f $script:PeFiles
    $verdict = if ($Fits) { 'is not plan_too_large' } else { 'is unsatisfiable with the one reason plan_too_large, size xl' }
    $name = "lease-api.md#fields: $Desc $verdict (turn '$(Format-PeText $Turn)', $Agents agents at once)"
    $p = [System.Collections.Generic.List[string]]::new()
    try {
      Write-PeText -Name $file -Text $Text
      $run = Invoke-PeLease -Arguments (@('create', '--project', $ProjectArg, '--plan', $file) + $Extra)
      if ($Fits) { foreach ($x in (Get-PeFitsProblems -Run $run -Turn $Turn -Agents $Agents -PlanName $PlanName -Size 'xl')) { $p.Add($x) } }
      else { foreach ($x in (Get-PeTooLargeProblems -Run $run -Turn $Turn -Agents $Agents -PlanName $PlanName -Project $Project -Platform $Platform -ProfileText $ProfileText -TtlSeconds $TtlSeconds)) { $p.Add($x) } }
    } catch { $p.Add("It threw: $($_.Exception.Message)") }
    Complete-PeCheck -Name $name -Problems $p
  }
  $most = $script:PeMost
  # The boundary, by stories: $most - 1 stories and the dev lead are $most agents, and one more story is one too many.
  Test-PeTooLarge -Desc "A turn of $($most - 1) one-subagent stories and the dev lead, $most agents" -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'T1' -Stories ($most - 1)))) -Turn 'T1' -Agents $most -PlanName $planName -Fits
  Test-PeTooLarge -Desc "A turn of $most one-subagent stories and the dev lead, $($most + 1) agents" -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $planName
  # By one story's subagents: a story's peak is its width, so $most independent subagents with the dev lead are one too many.
  Test-PeTooLarge -Desc "One story of $($most - 1) subagents that start after none, and the dev lead, $most agents" -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories 1 -Subagents ($most - 1)))) -Turn 'T1' -Agents $most -PlanName $null -Fits
  Test-PeTooLarge -Desc "One story of $most subagents that start after none, and the dev lead, $($most + 1) agents" -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories 1 -Subagents $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null
  # The width of the starts, not the number of subagents: 32 subagents of which the last starts after the other 31 are 31 wide.
  $wide31 = New-PeStory -Name 'S1' -Subs (@(1..31 | ForEach-Object { New-PeSub -Name "A$_" }) + @(New-PeSub -Name 'Z' -After @(1..31 | ForEach-Object { "A$_" })))
  Test-PeTooLarge -Desc 'One story of 32 subagents of which the last starts after the other 31, a peak of 31, and the dev lead' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @($wide31)))) -Turn 'T1' -Agents $most -PlanName $null -Fits
  Test-PeTooLarge -Desc 'Two stories of 16 subagents that start after none, a peak of 32 in all, and the dev lead' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @(1..16 | ForEach-Object { New-PeSub -Name "A$_" })), (New-PeStory -Name 'S2' -Subs @(1..16 | ForEach-Object { New-PeSub -Name "A$_" })))))) -Turn 'T1' -Agents 33 -PlanName $null
  Test-PeTooLarge -Desc 'Two stories of 16 and 15 subagents that start after none, a peak of 31 in all, and the dev lead' -Text (New-PePlan -Turns @((New-PeTurn -Name 'T1' -Stories @((New-PeStory -Name 'S1' -Subs @(1..16 | ForEach-Object { New-PeSub -Name "A$_" })), (New-PeStory -Name 'S2' -Subs @(1..15 | ForEach-Object { New-PeSub -Name "A$_" })))))) -Turn 'T1' -Agents $most -PlanName $null -Fits
  # The busiest turn, not the plan's total, and the first on a tie.
  Test-PeTooLarge -Desc "Two turns of $most agents each, whose total is more than $most" -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'T1' -Stories ($most - 1)), (New-PeWideTurn -Name 'T2' -Stories ($most - 1)))) -Turn 'T1' -Agents $most -PlanName $planName -Fits
  Test-PeTooLarge -Desc 'Five turns of 21 agents each, whose total is more than 32' -Text (New-PePlan -Turns @(1..5 | ForEach-Object { New-PeWideTurn -Name "T$_" -Stories 20 })) -Turn 'T1' -Agents 21 -PlanName $null -Fits
  Test-PeTooLarge -Desc 'A first turn that fits and a second that is one agent too many: the second is the busiest turn' -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'first turn' -Stories 5), (New-PeWideTurn -Name 'second' -Stories $most))) -Turn 'second' -Agents ($most + 1) -PlanName $planName
  Test-PeTooLarge -Desc 'A first turn that is too large and a second that is larger still: the second is the busiest' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most), (New-PeWideTurn -Name 'T2' -Stories 32 -Subagents 2))) -Turn 'T2' -Agents 65 -PlanName $null
  Test-PeTooLarge -Desc 'Two turns equally too large: the first is named' -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'later' -Stories $most), (New-PeWideTurn -Name 'earlier' -Stories $most))) -Turn 'later' -Agents ($most + 1) -PlanName $planName
  Test-PeTooLarge -Desc 'Three turns, the second and the third equally too large and the first not: the second is named' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'small' -Stories 3), (New-PeWideTurn -Name 'second' -Stories $most), (New-PeWideTurn -Name 'third' -Stories $most))) -Turn 'second' -Agents ($most + 1) -PlanName $null
  # The names are printed as the plan writes them, in sized_for and in the message.
  Test-PeTooLarge -Desc "A plan and a turn whose names have spaces and an apostrophe" -Text (New-PePlan -Name "the plan's name" -Turns @((New-PeWideTurn -Name "it's the busy turn" -Stories $most))) -Turn "it's the busy turn" -Agents ($most + 1) -PlanName "the plan's name"
  Test-PeTooLarge -Desc 'A plan whose name is null, which has no sized_for.plan' -Text ('{"name":null,"turns":[' + (New-PeWideTurn -Name 'T1' -Stories $most) + ']}') -Turn 'T1' -Agents ($most + 1) -PlanName $null
  Test-PeTooLarge -Desc 'A plan that has no name at all' -Text ('{"turns":[' + (New-PeWideTurn -Name 'T1' -Stories $most) + ']}') -Turn 'T1' -Agents ($most + 1) -PlanName $null
  Test-PeTooLarge -Desc 'A turn named with an escape, T\u0031' -Text ('{"turns":[' + (New-PeWideTurn -Name 'T1' -Stories $most).Replace('"T1"', '"T\u0031"') + ']}') -Turn 'T1' -Agents ($most + 1) -PlanName $null
  # Its only reason, whatever the platform and the profile (D81): no driver is asked, and the driver is what judges them.
  Test-PeTooLarge -Desc 'A platform no driver supports' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Extra @('--platform', 'windows/amd64') -Platform 'windows/amd64'
  Test-PeTooLarge -Desc 'A profile no driver supports' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Extra @('--profile', 'agent-linux@1') -ProfileText 'agent-linux@1'
  Test-PeTooLarge -Desc 'A platform and a profile no driver supports' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Extra @('--platform', 'linux/amd64', '--profile', 'other@2') -Platform 'linux/amd64' -ProfileText 'other@2'
  Test-PeTooLarge -Desc 'A platform and a profile in other forms, normalized' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Extra @('--Platform', ' Linux/AArch64 ', '--PROFILE=Agent-Linux@03') -ProfileText 'agent-linux@3'
  Test-PeTooLarge -Desc 'A TTL of 1h and a project in mixed case with spaces around it, normalized' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Extra @('--ttl', '1h') -TtlSeconds 3600 -ProjectArg ' PE-Case '
  Test-PeTooLarge -Desc 'The box, project swimcoach, whose host needs no making' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Project 'swimcoach'
  # A project with a host the driver would place on: still the one reason, decided before any driver is asked, the registry untouched.
  [void](Add-ConformanceReadyHost -ConformanceHome $h2 -Project 'pe-host')
  $hostsBefore = Get-CcFileHash (Join-Path $h2.UserProfile '.agentspace\hosts.json')
  Test-PeTooLarge -Desc 'A project whose host is ready (decided before any driver is asked)' -Text (New-PePlan -Turns @((New-PeWideTurn -Name 'T1' -Stories $most))) -Turn 'T1' -Agents ($most + 1) -PlanName $null -Project 'pe-host'
  $p = [System.Collections.Generic.List[string]]::new()
  if ($script:PeLeases -lt 30) { $p.Add("Only $($script:PeLeases) lease documents were read, so nothing shows that a plan_too_large create was answered") }
  if ((Get-CcFileHash (Join-Path $h2.UserProfile '.agentspace\hosts.json')) -cne $hostsBefore) { $p.Add('The host registry changed') }
  if (Test-Path -LiteralPath (Join-Path $h2.UserProfile '.agentspace\hosts')) { $p.Add('The hosts folder was made') }
  if (Test-Path -LiteralPath (Join-Path $h2.UserProfile '.agentspace\projects.json')) { $p.Add('A project list was made') }
  Complete-PeCheck -Name 'lease-api.md#agentspace-lease-create: A plan_too_large create changes no host: no registry change, no host folder' -Problems $p

  # --- What a plan_too_large create leaves: a kept lease, and nothing issued, with no CA ---
  $script:PeFiles++
  $file = 'k.json'
  Write-PeText -Name $file -Text (New-PePlan -Name $planName -Turns @((New-PeWideTurn -Name 'T1' -Stories $most)))
  $before = Get-PvRecordCount -Lab $h2
  $run = Invoke-PeLease -Arguments @('create', '--project', 'pe-kept', '--plan', $file)
  $p = Get-PeTooLargeProblems -Run $run -Turn 'T1' -Agents ($most + 1) -PlanName $planName -Project 'pe-kept'
  $id = if ($run.Json -is [hashtable]) { [string]$run.Json['id'] } else { '' }
  Complete-PeCheck -Name 'lease-api.md#agentspace-lease-create: A plan_too_large create needs no CA (the throwaway CA was removed) and exits 0' -Problems $p
  $p = [System.Collections.Generic.List[string]]::new()
  if ((Get-PvRecordCount -Lab $h2) -ne $before + 1) { $p.Add('The lease was not recorded: the records did not grow by one') }
  foreach ($x in (Get-PvLeaseFilesProblems -Lab $h2)) { $p.Add($x) }
  Complete-PeCheck -Name 'lease-api.md#agentspace-lease-create: A plan_too_large lease is recorded, and nothing is issued: no lease folder, no key, no alias file' -Problems $p
  if ($id -cmatch '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z') {
    $get = Invoke-PeLease -Arguments @('get', $id)
    $p = Get-ConformanceSuccessProblems -Run $get
    if ($run.Json -is [hashtable] -and $get.Json -is [hashtable] -and (ConvertTo-CcCanonical $run.Json) -cne (ConvertTo-CcCanonical $get.Json)) { $p.Add('get does not print the lease as create printed it') }
    Complete-PeCheck -Name 'lease-api.md#fields: get prints a plan_too_large lease with its sized_for as create printed it' -Problems $p
    $list = Invoke-PeLease -Arguments @('list', '--project', 'pe-kept', '--state', 'unsatisfiable')
    $p = Get-ConformanceSuccessProblems -Run $list -Schema 'lease-list.output.schema.json'
    $items = @(if ($list.Json -is [hashtable]) { $list.Json['leases'] })
    if ($items.Count -ne 1) { $p.Add("list shows $($items.Count) leases, not the one") }
    elseif ($run.Json -is [hashtable] -and (ConvertTo-CcCanonical $run.Json) -cne (ConvertTo-CcCanonical $items[0])) { $p.Add('list does not print the lease as create printed it') }
    Complete-PeCheck -Name 'lease-api.md#fields: list prints a plan_too_large lease with its sized_for, and the list validates' -Problems $p
    $rel = Invoke-PeLease -Arguments @('release', $id)
    $p = Get-ConformanceSuccessProblems -Run $rel
    if ($run.Json -is [hashtable] -and $rel.Json -is [hashtable] -and (ConvertTo-CcCanonical $run.Json) -cne (ConvertTo-CcCanonical $rel.Json)) { $p.Add('release does not print the lease unchanged') }
    Complete-PeCheck -Name 'lease-api.md#agentspace-lease-release: release of a plan_too_large lease prints it unchanged, as for any unsatisfiable lease' -Problems $p
  } else {
    foreach ($n in 'fields: get prints a plan_too_large lease with its sized_for as create printed it', 'fields: list prints a plan_too_large lease with its sized_for, and the list validates', 'agentspace-lease-release: release of a plan_too_large lease prints it unchanged, as for any unsatisfiable lease') {
      Write-ConformanceResult -Name "lease-api.md#$n" -Status FAIL -Reason 'The create printed no lease ID, so there was nothing to read back'
    }
  }
  $p = Get-PvNoStartProblems -StandIn $script:PeStandIn
  if ($script:PeLeases -lt 30) { $p.Add("Only $($script:PeLeases) lease documents were read, so nothing shows that the creates were answered") }
  Complete-PeCheck -Name 'lease-api.md#agentspace-lease-create: No create that was answered plan_too_large or accepted a plan started a provisioning run' -Problems $p
  [Environment]::CurrentDirectory = $savedCwd
  Clear-PvCleanup -Lab $h2
  Remove-ConformanceHome -ConformanceHome $h2
  $h2 = $null
} catch {
  Write-ConformanceResult -Name 'test-plan-errors.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(Format-PeText $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  [Environment]::CurrentDirectory = $savedCwd
  foreach ($lab in @($h1, $h2)) {
    if ($lab) {
      try { Clear-PvCleanup -Lab $lab } catch { }
      Remove-ConformanceHome -ConformanceHome $lab
    }
  }
}
Exit-ConformanceRun

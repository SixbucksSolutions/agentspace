#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Tests pin P13 of v0.12-T03 (E0.12-T03-S07, #0260), written from docs/operator.md, "The tests' seams":
# - With its seam unset, agentspace-idle-check.ps1, agentspace-patch.ps1 and agentspace-boot.ps1 run
#   only as SYSTEM (S-1-5-18), and install.ps1 only as agent. Otherwise each writes
#   `<script>: without its test seam it runs only as <account>; nothing was done`, `<script>` its file
#   name without .ps1, and exits 1, having done nothing.
# - On a host, shutdown and sshd are named by full path, C:\Windows\System32\shutdown.exe and
#   C:\Windows\System32\OpenSSH\sshd.exe, never by bare name.
#
# The cases, in this order, which is load-bearing:
# 1. Static, first: each guarded script is parsed with the PowerShell parser and never run. The rule,
#    from the scripts' shape: the first statement of its one block is `$ErrorActionPreference = 'Stop'`;
#    before the guard there are only helper definitions, the seam's detection in its exact form
#    ($seamNames holding the row's names in the table's order, $seamSet, $underSeam), and statements
#    that can have no effect; the guard is the first top-level `if` whose condition is the account
#    test in its exact form, with no else, and whose body is pure and ends `exit 1`. A statement is
#    pure when every command in it is Where-Object or a helper defined before the guard whose body is
#    pure, every method it calls is [Environment]::GetEnvironmentVariable, [string]::IsNullOrEmpty,
#    [System.Security.Principal.WindowsIdentity]::GetCurrent, EndsWith or [Console]::Error.WriteLine,
#    it redirects nothing to a file, uses no & or dot-source, and assigns only plain variables. So
#    nothing before the guard writes a file, makes a folder, starts a program, logs a line, or calls
#    the registry or Task Scheduler.
# 2. Refusal: each guarded script is run once, as a separate pwsh 7 process, as the laptop's own user,
#    with every one of its seam variables removed, in a throwaway home. It runs a script ONLY when that
#    script's static case passed earlier in the same run; otherwise it reports FAIL and runs nothing.
#    It asserts exit 1, the page's exact message as the whole of stderr, nothing on stdout, the
#    throwaway home unchanged byte for byte (but for PowerShell 7's own startup cache, whose name is
#    kept and whose bytes PowerShell rewrites at each start), and C:\ProgramData\agentspace unchanged
#    by its names and their count, never its contents.
# 3. Bare name: each box/windows script, parsed so comments are left out, starts neither program by
#    bare name: no command, and no string whose first word is shutdown, shutdown.exe, sshd or sshd.exe
#    unless it is the full path. The one string kept is a *-Service cmdlet's argument, which names the
#    sshd service and starts no program.
# 4. Full path: the scripts that start shutdown or sshd on a host name each by its full path.
#
# The PATH stand-in for shutdown.exe that windows-test-common.ps1 puts first on a child's PATH stays as
# a second net; no case's verdict relies on it. The script's folder is -ScriptDir (default: box/windows
# beside this folder), and -Only runs the cases whose names match it; a plant proof names a planted
# copy under target\s07fix\planted-*, and the static plant's proof MUST run with -Only '^Static'.
param([string]$ScriptDir = '', [string]$Only = '')

. (Join-Path $PSScriptRoot 'windows-test-common.ps1')
$global:WtOnly = $Only

$resolvedDir = Resolve-WtScriptDir $ScriptDir
$testHome = New-WtHome

# operator.md's rows: each guarded script, its account and its seam's names in the table's order.
$guarded = [ordered]@{
  'agentspace-idle-check' = @{ Account = 'SYSTEM'; Seam = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_IDLE_CONF', 'AGENTSPACE_IDLE_FACTS') }
  'agentspace-patch' = @{ Account = 'SYSTEM'; Seam = @('AGENTSPACE_PATCH_STATE', 'AGENTSPACE_PATCH_FACTS') }
  'agentspace-boot' = @{ Account = 'SYSTEM'; Seam = @('AGENTSPACE_IDLE_STATE', 'AGENTSPACE_PATCH_STATE') }
  'install' = @{ Account = 'agent'; Seam = @('AGENTSPACE_INSTALL_ONLY', 'AGENTSPACE_SSHD_CONFIG', 'AGENTSPACE_SSHD_TEST_PROGRAM') }
}
$allScripts = @('agentspace-boot.ps1', 'agentspace-idle-check.ps1', 'agentspace-patch-check.ps1', 'agentspace-patch.ps1', 'agentspace-tools.ps1', 'bootstrap.ps1', 'install.ps1', 'localswim.ps1')
$fullShutdown = 'C:\Windows\System32\shutdown.exe'
$fullSshd = 'C:\Windows\System32\OpenSSH\sshd.exe'

# The account test each guard's condition MUST be, whitespace aside.
$guardConditions = @{
  SYSTEM = '-not $underSeam -and [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value -ne ''S-1-5-18'''
  agent = '-not $underSeam -and -not [System.Security.Principal.WindowsIdentity]::GetCurrent().Name.EndsWith(''\agent'', [StringComparison]::OrdinalIgnoreCase)'
}
$seamSetText = '$seamSet = @($seamNames | Where-Object { -not [string]::IsNullOrEmpty([Environment]::GetEnvironmentVariable($_)) })'
$underSeamText = '$underSeam = ($seamSet.Count -eq $seamNames.Count)'

# Which scripts' static cases passed in this run; the refusal cases read it.
$script:WtStaticPassed = @{}
function Format-WtAstText { param([Parameter(Mandatory)]$Ast) return (($Ast.Extent.Text -replace '\s+', ' ').Trim()) }

function Format-WtAstWhere { param([Parameter(Mandatory)]$Ast) return ('line {0}' -f $Ast.Extent.StartLineNumber) }

# The AST node kinds a pure statement MAY hold; any other kind is an effect, or one this rule does not
# cover, and either way it is not allowed before the guard.
$script:WtPureKinds = @(
  'ScriptBlockAst', 'NamedBlockAst', 'ParamBlockAst', 'ParameterAst', 'TypeConstraintAst', 'StatementBlockAst',
  'PipelineAst', 'CommandExpressionAst', 'CommandAst', 'CommandParameterAst', 'AssignmentStatementAst',
  'IfStatementAst', 'ExitStatementAst', 'BinaryExpressionAst', 'UnaryExpressionAst', 'ConstantExpressionAst',
  'StringConstantExpressionAst', 'ExpandableStringExpressionAst', 'VariableExpressionAst', 'MemberExpressionAst',
  'InvokeMemberExpressionAst', 'TypeExpressionAst', 'ArrayExpressionAst', 'ArrayLiteralAst', 'ParenExpressionAst',
  'SubExpressionAst', 'ScriptBlockExpressionAst', 'ConvertExpressionAst', 'IndexExpressionAst'
)
# The static methods a pure statement MAY call, each with the type names it MAY be written with.
$script:WtPureStatic = @{
  'GetEnvironmentVariable' = @('Environment', 'System.Environment')
  'IsNullOrEmpty' = @('string', 'System.String')
  'GetCurrent' = @('System.Security.Principal.WindowsIdentity')
}

# Why an AST can have an effect, one reason an entry; none when it is pure. A helper it calls is
# checked by its body, every definition of that name before the guard, once.
function Get-WtImpurity {
  param([Parameter(Mandatory)]$Ast, [Parameter(Mandatory)][hashtable]$Helpers, [Parameter(Mandatory)][hashtable]$Seen)
  $why = [Collections.Generic.List[string]]::new()
  foreach ($node in $Ast.FindAll({ $true }, $true)) {
    $kind = $node.GetType().Name
    if ($script:WtPureKinds -notcontains $kind) { $why.Add(('{0}: a {1}' -f (Format-WtAstWhere $node), $kind)); continue }
    if ($kind -eq 'CommandAst') {
      if ($node.InvocationOperator -ne [System.Management.Automation.Language.TokenKind]::Unknown) { $why.Add(('{0}: a command run with & or .' -f (Format-WtAstWhere $node))); continue }
      $cmdName = $node.GetCommandName()
      if ([string]::IsNullOrEmpty($cmdName)) { $why.Add(('{0}: a command with no plain name' -f (Format-WtAstWhere $node))); continue }
      if ($Helpers.ContainsKey($cmdName)) {
        if (-not $Seen.ContainsKey($cmdName)) {
          $Seen[$cmdName] = $true
          foreach ($def in $Helpers[$cmdName]) {
            # Its parameters as its header declares them, whose defaults run at each call, then its body.
            foreach ($part in (@($def.Parameters) + @($def.Body))) {
              if ($null -eq $part) { continue }
              foreach ($inner in (Get-WtImpurity -Ast $part -Helpers $Helpers -Seen $Seen)) { $why.Add(('in {0}, {1}' -f $cmdName, $inner)) }
            }
          }
        }
        continue
      }
      if ($cmdName -ne 'Where-Object') { $why.Add(('{0}: the command {1}' -f (Format-WtAstWhere $node), $cmdName)) }
      continue
    }
    if ($kind -eq 'InvokeMemberExpressionAst') {
      if ($node.Member -isnot [System.Management.Automation.Language.StringConstantExpressionAst]) { $why.Add(('{0}: a method named at run time' -f (Format-WtAstWhere $node))); continue }
      $memberName = $node.Member.Value
      $allowed = $false
      if ($node.Static) {
        if ($node.Expression -is [System.Management.Automation.Language.TypeExpressionAst] -and $script:WtPureStatic.ContainsKey($memberName)) {
          $allowed = ($script:WtPureStatic[$memberName] -contains $node.Expression.TypeName.FullName)
        }
      } elseif ($memberName -eq 'EndsWith') {
        $allowed = $true
      } elseif ($memberName -eq 'WriteLine') {
        $target = $node.Expression
        $allowed = ($target -is [System.Management.Automation.Language.MemberExpressionAst] -and $target.Static -and
          $target.Expression -is [System.Management.Automation.Language.TypeExpressionAst] -and
          (@('Console', 'System.Console') -contains $target.Expression.TypeName.FullName) -and
          $target.Member -is [System.Management.Automation.Language.StringConstantExpressionAst] -and $target.Member.Value -eq 'Error')
      }
      if (-not $allowed) { $why.Add(('{0}: the method {1}' -f (Format-WtAstWhere $node), $memberName)) }
      continue
    }
    if ($kind -eq 'AssignmentStatementAst') {
      $left = $node.Left
      if ($left -isnot [System.Management.Automation.Language.VariableExpressionAst]) { $why.Add(('{0}: an assignment to something other than a plain variable' -f (Format-WtAstWhere $node))); continue }
      $vp = $left.VariablePath
      if (-not ($vp.IsUnscopedVariable -or $vp.IsLocal)) { $why.Add(('{0}: an assignment to {1}' -f (Format-WtAstWhere $node), $vp.UserPath)) }
      continue
    }
  }
  return $why.ToArray()
}
# The static rule's problems for one script; none when its guard comes before any statement that can
# have an effect. It parses the file and runs nothing.
function Get-WtGuardProblems {
  param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Account, [Parameter(Mandatory)][string[]]$SeamNames)
  $problems = [Collections.Generic.List[string]]::new()
  $tokens = $null
  $parseErrors = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$parseErrors)
  if (@($parseErrors).Count -gt 0) { $problems.Add(('It does not parse: {0}' -f $parseErrors[0].Message)); return $problems.ToArray() }
  if ($null -ne $ast.ParamBlock -or $null -ne $ast.BeginBlock -or $null -ne $ast.ProcessBlock -or $null -ne $ast.DynamicParamBlock) {
    $problems.Add('It MUST hold its statements in one block with no param, begin, process or dynamicparam block, which this rule does not cover')
  }
  if ($null -eq $ast.EndBlock) { $problems.Add('It has no statements'); return $problems.ToArray() }
  if ($null -ne $ast.EndBlock.Traps -and $ast.EndBlock.Traps.Count -gt 0) { $problems.Add('It MUST hold no trap, which could carry a run past a failed account test') }
  $statements = @($ast.EndBlock.Statements)

  # The guard: the first top-level if whose condition is the account test in its exact form.
  $wantCondition = $guardConditions[$Account]
  $guardAt = -1
  for ($i = 0; $i -lt $statements.Count; $i++) {
    $st = $statements[$i]
    if ($st -is [System.Management.Automation.Language.IfStatementAst] -and @($st.Clauses).Count -ge 1 -and (Format-WtAstText $st.Clauses[0].Item1) -ceq $wantCondition) { $guardAt = $i; break }
  }
  if ($guardAt -lt 0) { $problems.Add(('No top-level if tests the account as {0}' -f $wantCondition)); return $problems.ToArray() }
  $guard = $statements[$guardAt]

  # The helpers defined before the guard, every definition of each name.
  $helpers = @{}
  for ($i = 0; $i -lt $guardAt; $i++) {
    $st = $statements[$i]
    if ($st -is [System.Management.Automation.Language.FunctionDefinitionAst]) {
      if (-not $helpers.ContainsKey($st.Name)) { $helpers[$st.Name] = [Collections.Generic.List[object]]::new() }
      $helpers[$st.Name].Add($st)
    }
  }

  # Before the guard: the first statement, the seam's detection in its exact form, and purity.
  $firstOther = $null
  $seamNamesText = '$seamNames = @(''' + ($SeamNames -join ''', ''') + ''')'
  $wanted = @($seamNamesText, $seamSetText, $underSeamText)
  $foundAt = @(-1, -1, -1)
  $watched = @('seamNames', 'seamSet', 'underSeam', 'ErrorActionPreference')
  for ($i = 0; $i -lt $guardAt; $i++) {
    $st = $statements[$i]
    if ($st -is [System.Management.Automation.Language.FunctionDefinitionAst]) { continue }
    $text = Format-WtAstText $st
    if ($null -eq $firstOther) {
      $firstOther = $st
      if ($text -cne '$ErrorActionPreference = ''Stop''') { $problems.Add(('{0}: the first statement MUST be $ErrorActionPreference = ''Stop'', so a failed account test ends the run' -f (Format-WtAstWhere $st))) }
      continue
    }
    $k = [Array]::IndexOf($wanted, $text)
    if ($k -ge 0) {
      if ($foundAt[$k] -ge 0) { $problems.Add(('{0}: the seam''s detection is made twice' -f (Format-WtAstWhere $st))) }
      $foundAt[$k] = $i
      continue
    }
    foreach ($assign in $st.FindAll({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] }, $true)) {
      if ($assign.Left -is [System.Management.Automation.Language.VariableExpressionAst] -and $watched -contains $assign.Left.VariablePath.UserPath) {
        $problems.Add(('{0}: ${1} MUST be set only by the seam''s detection, or once to Stop' -f (Format-WtAstWhere $assign), $assign.Left.VariablePath.UserPath))
      }
    }
    foreach ($reason in (Get-WtImpurity -Ast $st -Helpers $helpers -Seen @{})) { $problems.Add(('Before the guard, {0}' -f $reason)) }
  }
  if ($null -eq $firstOther) { $problems.Add('Nothing comes before the guard: $ErrorActionPreference and the seam''s detection are missing') }
  $labels = @('$seamNames holding the row''s names in order', '$seamSet', '$underSeam')
  for ($k = 0; $k -lt 3; $k++) { if ($foundAt[$k] -lt 0) { $problems.Add(('Before the guard, the seam''s detection MUST set {0} in its exact form' -f $labels[$k])) } }
  if ($foundAt[0] -ge 0 -and $foundAt[1] -ge 0 -and $foundAt[2] -ge 0 -and -not ($foundAt[0] -lt $foundAt[1] -and $foundAt[1] -lt $foundAt[2])) {
    $problems.Add('The seam''s detection MUST set $seamNames, then $seamSet, then $underSeam')
  }

  # The guard itself: one clause, no else, a pure body that ends exit 1.
  if (@($guard.Clauses).Count -ne 1 -or $null -ne $guard.ElseClause) { $problems.Add(('{0}: the guard MUST have one clause and no else' -f (Format-WtAstWhere $guard))) }
  $body = @($guard.Clauses[0].Item2.Statements)
  $last = $null
  if ($body.Count -gt 0) { $last = $body[$body.Count - 1] }
  if ($null -eq $last -or $last -isnot [System.Management.Automation.Language.ExitStatementAst] -or $null -eq $last.Pipeline -or (Format-WtAstText $last.Pipeline) -cne '1') {
    $problems.Add(('{0}: the guard''s body MUST end exit 1' -f (Format-WtAstWhere $guard)))
  }
  foreach ($reason in (Get-WtImpurity -Ast $guard -Helpers $helpers -Seen @{})) { $problems.Add(('In the guard, {0}' -f $reason)) }
  return $problems.ToArray()
}
# Whether a string's first word is shutdown or sshd by bare name: its last path part is one of the four
# names, and the word is not the full path.
function Test-WtBareWord {
  param([AllowEmptyString()][string]$Value)
  $first = (($Value.Trim() -split '\s+')[0]).Trim([char]39, [char]34)
  $leaf = ($first -split '[\\/]')[-1]
  if ($leaf -notmatch '^(?i)(shutdown|sshd)(\.exe)?$') { return $false }
  return -not ([string]::Equals($first, $fullShutdown, [StringComparison]::OrdinalIgnoreCase) -or [string]::Equals($first, $fullSshd, [StringComparison]::OrdinalIgnoreCase))
}

# Each call of shutdown or sshd by bare name: a command, or a string, whose first word is one. Comments
# are not in the parse tree, so they are left out. A *-Service cmdlet's arguments are kept, since they
# name a service and start no program.
function Get-WtBareCalls {
  param([Parameter(Mandatory)][string]$Path)
  $tokens = $null
  $parseErrors = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$parseErrors)
  $hits = [Collections.Generic.List[string]]::new()
  if (@($parseErrors).Count -gt 0) { $hits.Add(('It does not parse: {0}' -f $parseErrors[0].Message)); return $hits.ToArray() }
  $strings = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.StringConstantExpressionAst] -or $n -is [System.Management.Automation.Language.ExpandableStringExpressionAst] }, $true)
  foreach ($s in $strings) {
    $parentCmd = $s.Parent
    if ($parentCmd -is [System.Management.Automation.Language.CommandAst]) {
      $cmdName = $parentCmd.GetCommandName()
      if ($parentCmd.CommandElements[0] -ne $s -and -not [string]::IsNullOrEmpty($cmdName) -and $cmdName -like '*-Service') { continue }
    }
    if (Test-WtBareWord $s.Value) { $hits.Add(('line {0}: {1}' -f $s.Extent.StartLineNumber, $s.Extent.Text)) }
  }
  return $hits.ToArray()
}

# The throwaway home's contents, byte for byte, but for the bytes of PowerShell 7's own startup cache,
# which every pwsh start rewrites before a script's first line runs; its name is kept.
$startupCache = '\home\AppData\Local\Microsoft\PowerShell\StartupProfileData-NonInteractive|'
function Get-WtHomeMark {
  $rows = @((Get-WtSnapshot -Path $testHome.Root) -split "`n" | ForEach-Object { if ($_.StartsWith($startupCache, [StringComparison]::OrdinalIgnoreCase)) { $startupCache + 'pwsh' } else { $_ } })
  return ($rows -join "`n")
}

# The names in the machine's agentspace folder and their count, never its contents.
function Get-WtMachineNames {
  $p = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'agentspace'
  if (-not (Test-Path -LiteralPath $p)) { return 'absent' }
  $names = @(Get-ChildItem -LiteralPath $p -Force -Name | Sort-Object)
  return ('{0}|{1}' -f $names.Count, ($names -join '/'))
}

try {
  $realBefore = Get-WtRealMark

  # --- 1. Static, first: the guard before any statement that can have an effect -----------------------------
  foreach ($scriptBase in $guarded.Keys) {
    $baseCopy = $scriptBase
    Test-WtCase ("Static: {0}.ps1 tests its account before any statement that can have an effect" -f $baseCopy) {
      $p = Join-Path $resolvedDir ($baseCopy + '.ps1')
      Assert-WtScriptExists -Path $p
      $found = @(Get-WtGuardProblems -Path $p -Account $guarded[$baseCopy].Account -SeamNames $guarded[$baseCopy].Seam)
      if ($found.Count -gt 0) { throw ($found -join '; ') }
      $script:WtStaticPassed[$baseCopy] = $true
    }
  }

  # --- 2. Refusal: run without its seam as the laptop's own user, only after its static case passed --------------
  # The laptop's own user is neither SYSTEM nor agent, by SID and by name; were it either, a run would
  # not show the refusal, so each live case is SKIPPED with the reason, and the name is never printed.
  $me = [System.Security.Principal.WindowsIdentity]::GetCurrent()
  $myLeaf = ($me.Name -split '\\')[-1]
  $notMe = $null
  if ($me.User.Value -eq 'S-1-5-18') { $notMe = 'The laptop''s user is SYSTEM by its SID' }
  elseif ($myLeaf -ieq 'SYSTEM') { $notMe = 'The laptop''s user is SYSTEM by its name' }
  elseif ($myLeaf -ieq 'agent') { $notMe = 'The laptop''s user is agent by its name' }
  $warmed = $false
  foreach ($scriptBase in $guarded.Keys) {
    $baseCopy = $scriptBase
    $caseName = "Refusal: {0}.ps1 without its seam, as the laptop's own user, exits 1 with its message and does nothing" -f $baseCopy
    if (-not [string]::IsNullOrEmpty($global:WtOnly) -and $caseName -notmatch $global:WtOnly) { continue }
    if ($null -ne $notMe) {
      Write-ConformanceResult -Name $caseName -Status SKIPPED -Reason ($notMe + ', the account a guarded script runs as, so a run would not show the refusal')
      continue
    }
    if (-not $warmed -and $script:WtStaticPassed[$baseCopy] -eq $true) {
      # PowerShell 7 writes its startup cache into a fresh home at its first start: one start that runs
      # nothing puts it there before the first before-and-after.
      $warm = Start-ConformanceProcess -FilePath $script:WtPwsh -Arguments @('-NoProfile', '-NonInteractive', '-Command', 'exit 0') -ChildEnvironment (Get-ConformanceChildEnvironment -ConformanceHome $testHome -Override @{}) -TimeoutSeconds 60 -WorkingDirectory $testHome.UserProfile
      $warmed = ($warm.ExitCode -eq 0)
    }
    Test-WtCase $caseName {
      # The gate: a script whose static case did not pass in this run is never run.
      Assert-Wt ($script:WtStaticPassed[$baseCopy] -eq $true) 'Not run: its static case did not pass in this run, so the script is not known to test its account first'
      Assert-Wt $warmed 'Not run: PowerShell 7 could not be started once in the throwaway home'
      $p = Join-Path $resolvedDir ($baseCopy + '.ps1')
      $seam = @{}
      foreach ($row in $guarded.Values) { foreach ($v in $row.Seam) { $seam[$v] = $null } }
      $want = '{0}: without its test seam it runs only as {1}; nothing was done' -f $baseCopy, $guarded[$baseCopy].Account
      $homeBefore = Get-WtHomeMark
      $machineBefore = Get-WtMachineNames
      $r = Invoke-WtScript -TestHome $testHome -ScriptPath $p -Seam $seam -TimeoutSeconds 60
      $homeAfter = Get-WtHomeMark
      $machineAfter = Get-WtMachineNames
      Assert-Wt (-not $r.TimedOut) 'The run MUST end at once, and it timed out'
      Assert-WtEqual $r.ExitCode 1 'Exit code'
      Assert-Wt ($r.StdoutBytes.Length -eq 0) ('stdout MUST be empty: {0}' -f (Format-WtBytes $r.StdoutBytes))
      Assert-WtEqual ($r.Stderr.TrimEnd("`r", "`n")) $want 'stderr, the whole of it'
      Assert-Wt ($homeAfter -ceq $homeBefore) 'The throwaway home MUST be unchanged'
      Assert-Wt ($machineAfter -ceq $machineBefore) ("The machine's agentspace folder MUST be unchanged: {0} before, {1} after" -f ($machineBefore -split '\|')[0], ($machineAfter -split '\|')[0])
    }
  }

  # --- 3. Bare name: no call of shutdown or sshd by bare name ----------------------------------------------------
  foreach ($scriptFile in $allScripts) {
    $fileCopy = $scriptFile
    Test-WtCase ("Bare name: {0} starts neither shutdown nor sshd by bare name" -f $fileCopy) {
      $p = Join-Path $resolvedDir $fileCopy
      Assert-WtScriptExists -Path $p
      $hits = @(Get-WtBareCalls -Path $p)
      Assert-Wt ($hits.Count -eq 0) ('By bare name: ' + ($hits -join '; '))
    }
  }

  # --- 4. Full path: what a host starts, it names by full path ----------------------------------------------------
  foreach ($pair in @(@('agentspace-idle-check.ps1', $fullShutdown), @('agentspace-patch.ps1', $fullShutdown), @('install.ps1', $fullSshd))) {
    $pairCopy = $pair
    Test-WtCase ("Full path: {0} names {1}" -f $pairCopy[0], $pairCopy[1]) {
      $p = Join-Path $resolvedDir $pairCopy[0]
      Assert-WtScriptExists -Path $p
      $tokens = $null
      $parseErrors = $null
      $ast = [System.Management.Automation.Language.Parser]::ParseFile($p, [ref]$tokens, [ref]$parseErrors)
      $wantPath = $pairCopy[1]
      $named = @($ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.StringConstantExpressionAst] -and [string]::Equals($n.Value, $wantPath, [StringComparison]::OrdinalIgnoreCase) }, $true))
      Assert-Wt ($named.Count -gt 0) ('{0} MUST name {1}' -f $pairCopy[0], $wantPath)
    }
  }

  Test-WtCase 'Seam: Nothing was written outside the throwaway home' { Assert-WtRealMarkUnchanged -Before $realBefore }
} finally {
  Remove-ConformanceHome -ConformanceHome $testHome
}

Exit-ConformanceRun

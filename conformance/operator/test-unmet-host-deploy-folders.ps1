#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's folder, one per command (docs/operator.md, "The operator's folder,
# one per command", "What the prototype took from its checkout", "Laptop files"; docs/state.md, "Files in
# the checkout"; D55; Q32). Card #0156 (E0.07-T04-S02): `deploy` and `sync` each write what they send
# into a new `%USERPROFILE%\.agentspace\operator\<hex>\` that their stand-in `aws` and `scp` see named,
# never one another's, and remove it as they end, after a failure too; a folder that cannot be written
# fails the command with its name; and the next command that makes its own folder removes a `<hex>`
# folder created more than 24 hours ago. `provision`'s part of these rows is another file's.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp`
# (conformance/operator/host-commands-common.ps1). At each call the stand-in `aws` records the template's
# path, whether it exists, its folder's name and the folders beside it, and the stand-in `scp` records
# the same for each local path it is given. The check writes a marker file in the operator's folder, so
# that a path the stand-ins see is shown to be under this home's operator folder. The folders' creation
# times are set by the check. PATH holds no go.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every ID is made up, and a failure reason never
# quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-deploy-folders.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'

Invoke-HostFile -FileName 'test-unmet-host-deploy-folders.ps1' -Body {
  param($fx)
  $idA = $script:IdA; $idNew = $script:IdNew
  $stack = 'agentspace-host-agentspace'
  $marker = Join-Path $fx.OperatorDir 'conformance-marker.txt'
  $recNone = @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -State 'creating' -LastAttempt '2026-01-01T00:00:00Z'))
  $doesNotExist = New-Step '' "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id $stack does not exist" 254
  $stackDone = New-Step (New-StackDoc -StackName $stack -Id $idNew -Status 'CREATE_COMPLETE')
  function Set-Marker {
    New-HomeFolder $fx.OperatorDir
    [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $marker), 'marker')
  }
  # A host that deploys and syncs to the end: a new stack, then a running instance launched long ago.
  function Set-FullFlow($StackSteps = @($doesNotExist, $stackDone), [hashtable]$More = @{}) {
    Set-HostsRecords $recNone
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ "cloudformation describe-stacks $stack" = @($StackSteps) } $More)
    Set-Marker
  }
  function Set-BoxFullFlow {
    Set-HostsRecords @((New-BoxRecord))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdNew) @{ 'cloudformation describe-stacks agentbox' = @((New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdNew))) })
    Set-Marker
  }
  function Set-SyncFlow {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdBox))
    Set-Marker
  }
  # The folders in the operator's folder whose names are 32 lowercase hex digits.
  function Get-HexFolders {
    if (-not (Test-Path -LiteralPath $fx.OperatorDir -PathType Container)) { return @() }
    @(Get-ChildItem -LiteralPath $fx.OperatorDir -Directory -Force | Where-Object { $_.Name -cmatch '^[0-9a-f]{32}$' } | ForEach-Object { $_.Name })
  }
  # What the deploy call saw, and what the scp of box/system saw.
  function Get-DeploySnapshot { $c = @(Get-AwsCalls | Where-Object { $_.Text -like 'cloudformation deploy*' }); if ($c.Count -gt 0) { $c[0].tpl } }
  function Get-SystemSnapshot {
    foreach ($c in @(Get-ScpCalls)) { foreach ($l in @($c.locals)) { if ((Split-Path -Leaf ([string]$l.path)) -ceq 'system') { return $l } } }
  }
  $folderProblems = {
    param([bool]$WithDeploy, [bool]$WithSync)
    $p = [System.Collections.Generic.List[string]]::new()
    $t = if ($WithDeploy) { Get-DeploySnapshot } else { $null }
    $sy = if ($WithSync) { Get-SystemSnapshot } else { $null }
    if ($WithDeploy) {
      if ($null -eq $t) { $p.Add('cloudformation deploy was not called with a --template-file') }
      else {
        if (-not $t.exists) { $p.Add('The template did not exist when cloudformation deploy was called') }
        if ($t.name -cne 'agentspace.yaml') { $p.Add('The template is not named agentspace.yaml') }
        if ($t.parentLeaf -cnotmatch '^[0-9a-f]{32}$') { $p.Add('The template''s folder is not named by 32 lowercase hex digits') }
        if ($t.opLeaf -cne 'operator' -or -not $t.marker) { $p.Add('The template''s folder is not directly in this home''s operator folder') }
        if ($t.size -le 0) { $p.Add('The template was empty') }
        if (@($t.siblings | Where-Object { $_ -cmatch '^[0-9a-f]{32}$' }).Count -ne 1) { $p.Add('More than one command folder existed when cloudformation deploy was called') }
      }
    }
    if ($WithSync) {
      if ($null -eq $sy) { $p.Add('No scp was given a local path named system') }
      else {
        if (-not $sy.exists -or -not $sy.isDir -or $sy.files -lt 1) { $p.Add('box\system was not a folder with files in it when it was copied') }
        if ($sy.parentLeaf -cnotmatch '^[0-9a-f]{32}$') { $p.Add('The copied system folder is not in a folder named by 32 lowercase hex digits') }
        if ($sy.opLeaf -cne 'operator' -or -not $sy.marker) { $p.Add('The copied system folder is not in a folder directly in this home''s operator folder') }
      }
    }
    if ($WithDeploy -and $WithSync -and $t -and $sy -and $t.parentLeaf -cne $sy.parentLeaf) { $p.Add('deploy''s sync step wrote into another folder than deploy''s: a command''s steps share one folder') }
    if (@(Get-HexFolders).Count -gt 0) { $p.Add('A command folder was left in the operator''s folder') }
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { $p.Add('Something removed what was not the command''s own: the marker file is gone') }
    $p.ToArray()
  }

  # ============ deploy ============
  $d = 'operator.md#the-operators-folder-one-per-command'
  Test-HostCase "${d}: deploy -Host <project> writes agentspace.yaml into a new 32-hex folder of the operator's folder before cloudformation deploy reads it, its sync step copies system from the same folder, and the folder is removed as the command ends" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $true $true } } -Setup { Set-FullFlow }
  Test-HostCase "${d}: the box's deploy writes its template into such a folder too, and removes it" @('deploy') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $true $true } } -Setup { Set-BoxFullFlow }
  Test-HostCase "${d}: a failed cloudformation deploy leaves no folder: it is removed as the command ends, whether it succeeded or failed" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'
    Check = { param($run, $started) & $folderProblems $true $false } } -Setup {
    Set-FullFlow -More @{ 'cloudformation deploy' = @((New-Step '' 'An error occurred (ValidationError) when calling the CreateChangeSet operation: no' 255)) }
  }
  Test-HostCase "${d}: a deploy that fails at step 5 (the stack has no InstanceId output) leaves no folder" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'
    Check = { param($run, $started) & $folderProblems $true $false } } -Setup {
    Set-FullFlow @($doesNotExist, (New-Step (New-StackDoc -StackName $stack)))
  }
  Test-HostCase "${d}: a deploy whose sync step fails leaves no folder either" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started) & $folderProblems $true $true } } -Setup {
    Set-FullFlow
    Set-ProgramAnswer scp -Code 1
  }
  Test-HostCase "${d}: two commands never share a folder: the folder of one deploy is not the folder of the next, and neither is left" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $first = (Get-DeploySnapshot).parentLeaf
      Clear-StandinLogs
      Set-FullFlow
      $r2 = Invoke-HostRun -Arguments @('deploy', '-Host', 'agentspace')
      $second = (Get-DeploySnapshot).parentLeaf
      if ($r2.ExitCode -ne 0) { 'The second deploy did not succeed' }
      if (-not $first -or -not $second) { 'A deploy call did not see a template folder' }
      elseif ($first -ceq $second) { 'Two commands wrote into the same folder name' }
      & $folderProblems $true $true } } -Setup { Set-FullFlow }

  # ============ sync ============
  $s = 'operator.md#the-operators-folder-one-per-command'
  Test-HostCase "${s}: sync -Host <project> writes box/system into a new 32-hex folder of the operator's folder before it copies it, and removes the folder as the command ends" @('sync', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $false $true } } -Setup { Set-SyncFlow }
  Test-HostCase "${s}: the box's sync does the same" @('sync') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $false $true } } -Setup { Set-SyncFlow }
  Test-HostCase "${s}: a sync that fails (the copy of box/system fails) removes its folder too" @('sync', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $false $true } } -Setup {
    Set-SyncFlow
    Set-ProgramAnswer scp -Code 1
  }
  Test-HostCase "${s}: a sync that fails at its install step removes its folder too" @('sync', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $folderProblems $false $true } } -Setup {
    Set-SyncFlow
    Set-ProgramAnswer ssh -Code 255 -On @(2)
  }
  Test-HostCase "${s}: two syncs never share a folder: the folder of one is not the folder of the next" @('sync', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
    Check = { param($run, $started)
      $first = (Get-SystemSnapshot).parentLeaf
      Clear-StandinLogs
      Set-SyncFlow
      $r2 = Invoke-HostRun -Arguments @('sync', '-Host', 'agentspace')
      $second = (Get-SystemSnapshot).parentLeaf
      if ($r2.ExitCode -ne 0) { 'The second sync did not succeed' }
      if (-not $first -or -not $second) { 'A scp did not see the system folder' }
      elseif ($first -ceq $second) { 'Two commands wrote into the same folder name' }
      & $folderProblems $false $true } } -Setup { Set-SyncFlow }

  # ============ A folder that cannot be written ============
  # Something that is not a folder at the operator's own path: the command fails before anything reads
  # a file it should have written, with Cannot write <that path>., never a path below it.
  $w = 'operator.md#what-the-prototype-took-from-its-checkout'
  $cannotWrite = {
    param($run)
    $m = [regex]::Match($run.Stderr, '(?m)^agentspace: Cannot write (.+)\.$')
    if (-not $m.Success) { return 'Stderr has no line agentspace: Cannot write <path>.' }
    if ($m.Groups[1].Value -ine $fx.OperatorDir -and $m.Groups[1].Value -ine (Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.OperatorDir)) { 'The path is not the operator folder itself (it names a folder or file below it, or another)' }
    $last = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })[-1]
    if ($last -cne $m.Value) { 'The failure is not the last line' }
    if (-not (Test-SameBytes (Get-FileBytesOrNull $fx.OperatorDir) ([Text.Encoding]::UTF8.GetBytes('not a folder')))) { 'The file at the operator folder''s path was changed or removed' }
  }
  $operatorIsAFile = { Remove-HomePath $fx.OperatorDir; [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $fx.OperatorDir), 'not a folder') }
  Test-HostCase "${w}: deploy -Host <project> with a file where the operator's folder should be fails with Cannot write <the operator folder>., before cloudformation deploy reads a template" @('deploy', '-Host', 'agentspace') @{
    Exit = 1; Calls = @((Get-CallStack $stack)); Check = { param($run, $started) & $cannotWrite $run } } -Setup {
    Set-HostsRecords $recNone
    Set-AwsScenario @{ "cloudformation describe-stacks $stack" = @((New-Step (New-StackDoc -StackName $stack -Id $script:IdNew -Status 'UPDATE_COMPLETE'))) }
    & $operatorIsAFile
  }
  Test-HostCase "${w}: the box's deploy with such a file fails the same way, with no cloudformation deploy call" @('deploy') @{
    Exit = 1; Calls = @(); Check = { param($run, $started) & $cannotWrite $run } } -Setup {
    Set-HostsRecords @((New-BoxRecord))
    & $operatorIsAFile
  }
  Test-HostCase "${w}: sync -Host <project> with such a file fails the same way, and copies nothing" @('sync', '-Host', 'agentspace') @{
    Exit = 1; Calls = 'any'; SshCalls = 'any'; Check = { param($run, $started) & $cannotWrite $run } } -Setup {
    Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA)
    & $operatorIsAFile
  }

  # ============ The 24-hour sweep ============
  $sw = 'operator.md#the-operators-folder-one-per-command'
  $hexOld = '0123456789abcdef0123456789abcdef'
  $hexYoung = 'fedcba9876543210fedcba9876543210'
  $hexMtimeOld = 'aaaabbbbccccddddeeeeffff00001111'
  $upper = 'ABCDEF0123456789ABCDEF0123456789'
  $short31 = '0123456789abcdef0123456789abcde'
  $long33 = '0123456789abcdef0123456789abcdef0'
  $plantFolders = {
    Set-Marker
    $now = [DateTime]::UtcNow
    foreach ($spec in @(
        @($hexOld, $now.AddHours(-25)), @($hexYoung, $now.AddHours(-23)), @($hexMtimeOld, $now.AddHours(-1)),
        @($upper, $now.AddHours(-48)), @($short31, $now.AddHours(-48)), @($long33, $now.AddHours(-48)),
        @('system', $now.AddHours(-48)), @('older-folder', $now.AddHours(-72)))) {
      $dir = Join-Path $fx.OperatorDir $spec[0]
      New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $fx.H -Path $dir) | Out-Null
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $dir 'file.txt')), 'x')
      [IO.Directory]::SetCreationTimeUtc((Assert-ConformancePath -ConformanceHome $fx.H -Path $dir), $spec[1])
    }
    # This one was created an hour ago and last written two days ago: the measure is the creation time.
    [IO.Directory]::SetLastWriteTimeUtc((Assert-ConformancePath -ConformanceHome $fx.H -Path (Join-Path $fx.OperatorDir $hexMtimeOld)), $now.AddHours(-48))
    foreach ($f in 'agentspace.yaml', 'localswim-linux-arm64') {
      $p = Join-Path $fx.OperatorDir $f
      [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $fx.H -Path $p), 'a v0.06 file')
      [IO.File]::SetCreationTimeUtc((Assert-ConformancePath -ConformanceHome $fx.H -Path $p), $now.AddHours(-72))
    }
  }
  $sweepProblems = {
    param([bool]$ExpectSwept)
    $p = [System.Collections.Generic.List[string]]::new()
    $has = { param($n) Test-Path -LiteralPath (Join-Path $fx.OperatorDir $n) }
    if ($ExpectSwept) {
      if (& $has $hexOld) { $p.Add('A folder created 25 hours ago was not removed') }
    } else {
      if (-not (& $has $hexOld)) { $p.Add('A command that makes no folder of its own removed a folder created 25 hours ago') }
    }
    if (-not (& $has $hexYoung)) { $p.Add('A folder created 23 hours ago was removed') }
    if (-not (& $has $hexMtimeOld)) { $p.Add('A folder created an hour ago but last written two days ago was removed: the measure is the creation time') }
    foreach ($n in $upper, $short31, $long33, 'system', 'older-folder', 'agentspace.yaml', 'localswim-linux-arm64', 'conformance-marker.txt') {
      if (-not (& $has $n)) { $p.Add("'$n' was removed, though it is not a folder of 32 lowercase hex digits (anything else there is left)") }
    }
    $p.ToArray()
  }
  Test-HostCase "${sw}: a deploy that makes its folder removes every 32-hex folder created more than 24 hours ago, and leaves a younger one, one created recently, and anything of another name, a v0.06 operator's files included" @('deploy', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $sweepProblems $true; if (@(Get-HexFolders).Count -ne 2) { 'The command''s own folder was left, or a young folder was removed' } } } -Setup {
    Set-FullFlow
    & $plantFolders
  }
  Test-HostCase "${sw}: a sync that makes its folder sweeps the same way" @('sync', '-Host', 'agentspace') @{
    Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'; Check = { param($run, $started) & $sweepProblems $true } } -Setup {
    Set-SyncFlow
    & $plantFolders
  }
  foreach ($case in @(
      @('status', @('status', '-Host', 'agentspace')),
      @('down', @('down', '-Host', 'agentspace')),
      @('up', @('up', '-Host', 'agentspace')),
      @('bootstrap', @('bootstrap', '-Host', 'agentspace')),
      @('grow of a stopped host', @('grow', '-Host', 'agentspace'))
    )) {
    $stopped = $case[0] -like 'grow*'
    Test-HostCase "${sw}: $($case[0]) sends no file, makes no folder of its own, and so sweeps nothing" ([string[]]$case[1]) @{
      Exit = 0; Calls = 'any'; SshCalls = 'any'; ScpCalls = 'any'
      Check = { param($run, $started) & $sweepProblems $false; if (@(Get-HexFolders).Count -ne 3) { 'A command folder was made, or a planted folder was removed' } } } -Setup {
      Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
      Set-AwsScenario (Get-HealthyAnswers -Id $script:IdA -State $(if ($stopped) { 'stopped' } else { 'running' }))
      & $plantFolders
    }
  }
}
Exit-ConformanceRun

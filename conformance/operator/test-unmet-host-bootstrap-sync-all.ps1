#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `sync -All` (docs/operator.md, "Every host at once: -All", "sync", "The operator's
# folder, one per command", "AWS failures", "Which checks show it"), run through the test seam. Card
# #0156 (E0.07-T04-S02). `status -All` and `down -All` are the commands writer's.
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `-All`: every case reports a FAIL there. A v0.07
# build MUST pass every result line.
#
# What it does: each case makes its own throwaway home holding a host registry of five records in this
# order: the box (ready), alpha (ready), beta (creating, no instance ID), gamma (failed, with an instance
# ID) and delta (ready); a CLAUDE.md; and the stand-ins of conformance/operator/host-provision-common.ps1.
# The order is not alphabetical, so a build that sorts shows it. `go` is found through a PATH of the
# check's own: a stand-in, or none, so no case starts the real `go`. sync's step 7 builds only when
# C:\Projects\localswim exists (operator.md, sync, step 7), and the `go` stand-in runs with that folder as its
# working folder, so a case that shows the build MUST report SKIPPED, naming the folder, when the folder
# is absent. That is one Test-Path -PathType Container on it; no case lists, reads or writes anything in it.
#
#   1. The order and the Host lines (`Host      <project> (<stack>), <state>`), the skips for a host
#      that is not ready, every host's calls in one block and none for a skipped host, the closing
#      count and exit code of a run that failed no host (none, 0).
#   2. `system\` written and the localswim CLI built once: one `go`, one build message, one operator\<hex>\
#      folder that every host's copies name, removed at the end; with no `go` the skip message once.
#   3. A host that fails ends its part and the command goes on: the closing `agentspace: <n> of <m> hosts
#      failed.` with <m> every record, exit 1; a build that fails fails every synced host's part with
#      the same message and builds no more.
#   4. #0034's sign-in message ends the command at once, with no closing count.
#   5. `-Quiet`: the Host lines still print and the skip lines do not; a stopped ready host is started
#      and none is stopped.
#
# SAFETY. See test-unmet-host-bootstrap-sync.ps1. A failure reason never quotes the program's output.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-bootstrap-sync-all.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#every-host-at-once--all'
$sy = 'operator.md#sync'
$hostLines = @(
  'Host      swimcoach (agentbox), ready',
  'Host      alpha (agentspace-host-alpha), ready',
  'Host      beta (agentspace-host-beta), creating',
  'Host      gamma (agentspace-host-gamma), failed',
  'Host      delta (agentspace-host-delta), ready')
$skipBeta = 'Skipping: the host is creating; its next provisioning run syncs it.'
$skipGamma = 'Skipping: the host is failed; its next provisioning run syncs it.'
$skipGo = 'Skipping the localswim CLI: Go or C:\Projects\localswim is missing on the laptop.'
$building = 'Building the localswim CLI for linux/arm64...'
$buildFailed = 'agentspace: Building the localswim CLI failed.'
$aliasAlpha = 'agentspace-host.alpha'
$aliasDelta = 'agentspace-host.delta'

function Initialize-AllHome($H, $S, [hashtable]$Aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) }, $Rules = @(), $Go = @{}) {
  $records = @(
    (New-HpRecord 'alpha' 'ready' $script:HpIdA '2026-01-01T00:00:00Z'),
    (New-HpRecord 'beta' 'creating' '' '2026-01-02T00:00:00Z'),
    (New-HpRecord 'gamma' 'failed' $script:HpIdB '2026-01-03T00:00:00Z'),
    (New-HpRecord 'delta' 'ready' $script:HpHostId '2026-01-04T00:00:00Z'))
  Initialize-HpHome -H $H -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records $records)
  New-HpClaudeMd $H
  Set-HpScenario -H $H -S $S -Scenario @{ aws = $Aws; rules = @($Rules); go = $Go }
}
function Test-Same([string[]]$A, [string[]]$B) { $A.Count -eq $B.Count -and (($A -join [char]0) -ceq ($B -join [char]0)) }
# The calls reduced to the host each one is for, consecutive repeats dropped: an AWS call by its
# instance ID, an ssh or scp by its alias, and `go` not at all.
function Get-Blocks($Calls) {
  $out = [System.Collections.Generic.List[string]]::new()
  foreach ($c in $Calls) {
    $k = $null
    if ($c['kind'] -eq 'aws') { $i = [Array]::IndexOf($c['a'], '--instance-ids'); if ($i -ge 0) { $k = 'id:' + $c['a'][$i + 1] } }
    elseif ($c['kind'] -in 'ssh', 'scp') { $k = 'alias:' + $c['alias'] }
    if ($k -and ($out.Count -eq 0 -or $out[$out.Count - 1] -cne $k)) { $out.Add($k) }
  }
  , @($out)
}
function Get-HostLineIndexes($Lines) { @($hostLines | ForEach-Object { [Array]::IndexOf($Lines, $_) }) }
$allBlocks = @("id:$($script:HpBoxId)", 'alias:agentspace', "id:$($script:HpIdA)", "alias:$aliasAlpha", "id:$($script:HpHostId)", "alias:$aliasDelta")

# ---- 1 and 2. A run that fails no host --------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: sync -All over five records" -NeedsLocalswim -Body {
  param($h, $s)
  Initialize-AllHome $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All') -WithGo
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr

  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  $idx = @(Get-HostLineIndexes $lines)
  if (($idx | Where-Object { $_ -lt 0 }).Count -gt 0) { $p.Add('stderr lacks a Host line of the form Host      <project> (<stack>), <state>') }
  elseif ((($idx | Sort-Object) -join ',') -cne ($idx -join ',')) { $p.Add('The Host lines are not in the registry''s order') }
  $iBeta = [Array]::IndexOf($lines, $skipBeta)
  $iGamma = [Array]::IndexOf($lines, $skipGamma)
  if ($idx.Count -eq 5 -and ($idx[2] -lt 0 -or $iBeta -ne $idx[2] + 1)) { $p.Add('The skip line for the creating host does not follow its Host line') }
  if ($idx.Count -eq 5 -and ($idx[3] -lt 0 -or $iGamma -ne $idx[3] + 1)) { $p.Add('The skip line for the failed host does not follow its Host line') }
  if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { $p.Add('A closing count was printed though no host failed') }
  Write-HpProblems "${sec}: sync -All prints Host      <project> (<stack>), <state> before each host in the registry's file order, a Skipping line for each host that is not ready, no closing count and exit 0 when none failed" $p

  $p = New-HpProblems
  $blocks = Get-Blocks $calls
  if (-not (Test-Same $blocks $allBlocks)) { $p.Add("The calls are not one block per synced host in the registry's order (the box, alpha, delta), with no call for beta or gamma: $($blocks -join ' ')") }
  Write-HpProblems "${sec}: sync -All syncs only the ready hosts, each host's calls in one block through its own alias, each AWS call naming one instance, and no call for a creating or failed host (Every host at once; Calls by command)" $p

  $p = New-HpProblems
  $goCalls = @($calls | Where-Object { $_['kind'] -eq 'go' })
  $scp = @($calls | Where-Object { $_['kind'] -eq 'scp' })
  $systems = @($scp | Where-Object { $_['a'] -contains '-r' })
  $cli = @($scp | Where-Object { [string]$_['a'][-1] -like '*:/tmp/localswim' })
  if ($goCalls.Count -ne 1) { $p.Add("go was started $($goCalls.Count) times, not once") }
  if (@($lines | Where-Object { $_ -ceq $building }).Count -ne 1) { $p.Add('The build message is not printed once, in the first host''s part') }
  if ($lines -ccontains $skipGo) { $p.Add('The skip message was printed though go built') }
  if ($systems.Count -ne 3 -or $cli.Count -ne 3) { $p.Add("There are $($systems.Count) system copies and $($cli.Count) localswim copies, not one of each for each of three hosts") }
  else {
    $sysPaths = @($systems | ForEach-Object { $_['a'][4] } | Select-Object -Unique)
    $cliPaths = @($cli | ForEach-Object { $_['a'][3] } | Select-Object -Unique)
    if ($sysPaths.Count -ne 1 -or $cliPaths.Count -ne 1) { $p.Add('The hosts do not all copy the same system folder and the same build') }
    elseif ($null -eq (Get-HpOperatorHex $h $sysPaths[0]) -or (Get-HpOperatorHex $h $sysPaths[0]) -cne (Get-HpOperatorHex $h $cliPaths[0])) { $p.Add('The system folder and the build are not in one operator\<hex>\ folder') }
    $map = Get-HpRepoSystemMap
    foreach ($sc in $systems) {
      $sys = @($sc['paths'])[0]
      if ($null -eq $sys -or $sys['type'] -ne 'dir' -or (($sys['files'].Keys | Sort-Object) -join ',') -cne (($map.Keys | Sort-Object) -join ',')) { $p.Add('A host''s copy of the system folder was not box/system''s files at the time of the copy: it is written once and kept for every host'); break }
    }
    foreach ($sc in $cli) { $b = @($sc['paths'])[0]; if ($null -eq $b -or $b['type'] -ne 'file') { $p.Add('A host''s copy of the build was not a file at the time of the copy'); break } }
  }
  $hexSeen = @($calls | ForEach-Object { $_['opdirs'].Keys } | Select-Object -Unique)
  if ($hexSeen.Count -ne 1) { $p.Add("The stand-ins saw $($hexSeen.Count) operator\<hex>\ folders, not the command's one") }
  if (@(Get-HpOperatorFolders $h).Count -ne 0) { $p.Add('A folder under operator\ was left after the command ended') }
  Write-HpProblems "${sy}: sync -All writes system\ and builds the localswim CLI once, in one operator\<hex>\ folder that every host's copies name, the build message printed in the first part alone, and the folder removed at the end (Every host at once; The operator's folder, one per command)" $p

  $p = New-HpProblems
  foreach ($proj in 'alpha', 'delta') {
    foreach ($f in 'id_ed25519', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h $proj) $f) -PathType Leaf)) { $p.Add("hosts\$proj\$f was not made for a host that sync -All synced") } }
    if (-not (Test-Path -LiteralPath (Get-HpAliasFile $h "agentspace-host.$proj") -PathType Leaf)) { $p.Add("The operator alias of $proj was not written") }
  }
  foreach ($proj in 'beta', 'gamma', 'swimcoach') { if (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h $proj) 'id_ed25519')) { $p.Add("A key was made for $proj, which sync -All did not sync, or for the box") } }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $h)) $hostsBefore)) { $p.Add('The registry was written') }
  Write-HpProblems "${sec}: sync -All signs and writes an alias for each host that is not the box that it syncs, none for a skipped host or the box, and changes no record (How the operator reaches a host)" $p
}

Invoke-HpInHome -Name "${sec}: sync -All with no go" -Body {
  param($h, $s)
  Initialize-AllHome $h $s
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All')
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if (@($lines | Where-Object { $_ -ceq $skipGo }).Count -ne 1) { $p.Add('The skip message is not printed once, in the first host''s part') }
  if (@($calls | Where-Object { $_['kind'] -eq 'go' }).Count -gt 0) { $p.Add('go was started') }
  if (@($calls | Where-Object { $_['kind'] -eq 'scp' -and [string]$_['a'][-1] -like '*:/tmp/localswim' }).Count -gt 0) { $p.Add('A localswim build was copied though none was made') }
  if (-not (Test-Same (Get-Blocks $calls) $allBlocks)) { $p.Add('The ready hosts were not all synced') }
  Write-HpProblems "${sec}: with no go the skip message is printed once, in the first host's part, the hosts are all synced, and it is no failure (Every host at once; sync, step 7)" $p
}

# ---- 3. A host that fails ends its part and the run goes on --------------------------------------------------------
Invoke-HpInHome -Name "${sec}: a host that fails" -NeedsLocalswim -Body {
  param($h, $s)
  Initialize-AllHome $h $s -Rules @(@{ kind = 'ssh'; class = 'sync'; alias = $aliasAlpha; nth = 2; code = 1 })
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All') -WithGo
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  $idx = @(Get-HostLineIndexes $lines)
  $iFail = [Array]::IndexOf($lines, 'agentspace: Install on the instance failed.')
  if ($idx.Count -ne 5 -or ($idx | Where-Object { $_ -lt 0 }).Count -gt 0) { $p.Add('stderr lacks a Host line') }
  elseif (-not ($idx[1] -lt $iFail -and $iFail -lt $idx[2])) { $p.Add('The failure is not printed in alpha''s part, between its Host line and the next') }
  if ($lines.Count -eq 0 -or $lines[-1] -cne 'agentspace: 1 of 5 hosts failed.') { $p.Add('The last line of stderr is not agentspace: 1 of 5 hosts failed., with 5 the number of records') }
  $alphaCalls = @($calls | Where-Object { $_['alias'] -eq $aliasAlpha })
  if ($alphaCalls.Count -ne 4) { $p.Add("alpha's part made $($alphaCalls.Count) ssh and scp calls, not the 4 up to and including its failed install") }
  if (@($calls | Where-Object { $_['alias'] -eq $aliasDelta }).Count -lt 5) { $p.Add('The command did not go on to delta after alpha''s failure') }
  Write-HpProblems "${sec}: a failure on one host ends that host's part and the command goes on to the next: the failure in its part, the last line agentspace: <n> of <m> hosts failed. with <m> the number of records, exit 1 (Every host at once; Exit codes)" $p
}

Invoke-HpInHome -Name "${sec}: a localswim build that fails" -NeedsLocalswim -Body {
  param($h, $s)
  Initialize-AllHome $h $s -Go @{ code = 1; err = "go build wrote this`n" }
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All') -WithGo
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if (@($calls | Where-Object { $_['kind'] -eq 'go' }).Count -ne 1) { $p.Add('go was started more than once: a build that failed is not built again for the later hosts') }
  if (@($lines | Where-Object { $_ -ceq $building }).Count -ne 1) { $p.Add('The build message is not printed once') }
  if (@($lines | Where-Object { $_ -ceq $buildFailed }).Count -ne 3) { $p.Add('The build''s failure is not the message of each of the three synced hosts'' parts, at the same step') }
  if ($lines -cnotcontains 'go build wrote this') { $p.Add('go''s own output did not pass through') }
  if (@($calls | Where-Object { $_['kind'] -eq 'scp' -and [string]$_['a'][-1] -like '*:/tmp/localswim' }).Count -gt 0) { $p.Add('A localswim build was copied though it failed') }
  if ($lines.Count -eq 0 -or $lines[-1] -cne 'agentspace: 3 of 5 hosts failed.') { $p.Add('The last line of stderr is not agentspace: 3 of 5 hosts failed.') }
  $iFail = @(0..($lines.Count - 1) | Where-Object { $lines[$_] -ceq $buildFailed })
  $idx = @(Get-HostLineIndexes $lines)
  if ($iFail.Count -eq 3 -and $idx.Count -eq 5 -and -not ($idx[0] -lt $iFail[0] -and $iFail[0] -lt $idx[1] -and $idx[1] -lt $iFail[1] -and $iFail[1] -lt $idx[4] -and $idx[4] -lt $iFail[2])) { $p.Add('The failures are not each in its own host''s part') }
  Write-HpProblems "${sec}: a build that fails fails that host's part and every later host's part at the same step with the same message, without building again, and the closing count is of the hosts that failed (Every host at once)" $p
}

# ---- 4. The sign-in failure ends the command ------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: an expired sign-in" -NeedsLocalswim -Body {
  param($h, $s)
  $signInErr = 'Unable to locate credentials. You can configure credentials by running "aws login".'
  Initialize-AllHome $h $s -Aws @{ 'ec2 describe-instances' = @((New-HpDescribe), (New-HpDescribe), (New-HpStep '' $signInErr 255)) }
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All') -WithGo
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if ($lines.Count -eq 0 -or $lines[-1] -cne $script:HpSignIn) { $p.Add('The last line of stderr is not #0034''s sign-in message') }
  if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { $p.Add('A closing count was printed: the sign-in failure ends the command at once') }
  if ($lines -ccontains $hostLines[4] -or $lines -ccontains $hostLines[2]) { $p.Add('The command went on to a later host after the sign-in failure') }
  if (@($calls | Where-Object { $_['alias'] -eq $aliasDelta -or ($_['a'] -contains $script:HpHostId) }).Count -gt 0) { $p.Add('A call was made for a later host after the sign-in failure') }
  if ($lines -cnotcontains $hostLines[0] -or $lines -cnotcontains $hostLines[1]) { $p.Add('The Host lines of the box and of alpha, where it failed, are not printed') }
  Write-HpProblems "${sec}: #0034's sign-in message ends sync -All at once: no later host is touched and no closing count is printed, exit 1 (Every host at once; AWS failures)" $p
}

# ---- 5. -Quiet, and a stopped host ---------------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: -Quiet" -NeedsLocalswim -Body {
  param($h, $s)
  Initialize-AllHome $h $s
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All', '-Quiet') -WithGo
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  foreach ($l in $hostLines) { if ($lines -cnotcontains $l) { $p.Add('A Host line is not printed with -Quiet: it always prints'); break } }
  if ($lines | Where-Object { $_ -like 'Skipping:*' -or $_ -like 'Building *' -or $_ -like 'Copying *' }) { $p.Add('-Quiet left a progress message') }
  Write-HpProblems "${sec}: -Quiet still prints the Host line of each host and suppresses the progress lines, the skips included (Messages)" $p
}

Invoke-HpInHome -Name "${sec}: sync -All starts a stopped ready host and stops none" -Body {
  param($h, $s)
  $aws = @{
    'ec2 describe-instances' = @((New-HpDescribe), (New-HpDescribe), (New-HpDescribe 'stopped'), (New-HpDescribe))
    'ssm describe-instance-information' = @((New-HpStep '{"InstanceInformationList":[{"PingStatus":"Online"}]}'))
    'ssm send-command' = @((New-HpStep '{"Command":{"CommandId":"c0ffee00-1111-4222-8333-abcdefabcdef"}}'))
    'ssm get-command-invocation' = @((New-HpStep "{`"Status`":`"Success`",`"StandardOutputContent`":`"patched\n`"}"))
  }
  Initialize-AllHome $h $s -Aws $aws
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-All')
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $starts = @($calls | Where-Object { (Get-HpCallName $_) -eq 'aws ec2 start-instances' })
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($starts.Count -ne 1 -or $starts[0]['a'] -notcontains $script:HpIdA) { $p.Add('The stopped ready host was not started once, by its own instance ID') }
  if (@($calls | Where-Object { (Get-HpCallName $_) -eq 'aws ec2 stop-instances' }).Count -gt 0) { $p.Add('sync -All stopped an instance') }
  if ($lines -cnotcontains "Starting $($script:HpIdA)...") { $p.Add('stderr lacks Starting <id>...') }
  Write-HpProblems "${sec}: sync -All starts every ready host that is stopped, as sync does, and stops none (Every host at once)" $p
}

Exit-ConformanceRun

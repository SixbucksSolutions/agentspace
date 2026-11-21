#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `provision`'s step 9, the EC2 driver's `place_pending` that a finished run calls
# (docs/operator.md, "provision", step 9 and "Its log"; docs/driver.md, "place_pending"; docs/state.md,
# "The lease state file", "The lease folder", "The alias file", "A lease alias for a host that is not the
# box"; "How each program finds the others"; "Which checks show it"), run through the test seam.
# Card #0156 (E0.07-T04-S02).
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `provision`: every case reports a FAIL there.
# A v0.07 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home with the stand-ins of
# conformance/operator/host-provision-common.ps1, a lease state file seeded with `pending` leases of the
# project (and one of another project and one `released`), and runs `provision -Host agentspace` against a
# healthy stand-in `aws`, `ssh` and `scp`, so that the run reaches step 9. It then checks the lease
# state file, the lease folders, the alias files, the log's `place` lines, the warning and the exit code.
#
#   1. A host made by the run: place_pending is called after `ready`, places every `pending` lease of the
#      project (state `ready`, `expires_at` a TTL after the placement, `ssh` the alias and `ubuntu`, no
#      other member changed), leaves another project's lease and a released one as they were, writes the
#      key, certificate and alias, the alias in the form for a host that is not the box and with the
#      default ProxyCommand line naming agentspace-ssh-proxy.exe beside the run's own executable, and
#      logs `place done placed=<n>`.
#   2. A killed placement's leftover files (a key, a certificate and an alias file naming the lease) are
#      removed first, and the lease is placed.
#   3. A lease whose alias is a foreign file stays `pending` with `place failed lease=<id> reason=<msg>`
#      logged after `place done placed=<n>`, the foreign file untouched, and the others placed.
#   4. A lease released while the run was working is never placed, and its record is not changed.
#   5. The operator run from a folder without agentspace-ssh-proxy.exe (the operator alone, copied under
#      the home). The box's run, the one that reaches step 9 from such a folder, since on a host that is
#      not the box the run fails before step 9, at step 6's operator alias, which needs the same program
#      (operator.md, provision, step 9): every lease of the project stays `pending`, each with a
#      `place failed lease=<id>` line, the call succeeds and the host stays `ready`; with
#      AGENTSPACE_PROXY_COMMAND set the leases are placed with that line and nothing is looked for; with an
#      agentspace-ssh-proxy.cmd beside the operator they are placed with the interpreter form. Where a
#      lease that fails is not the only one of its project (operator.md's "the others placed"), case 3's
#      foreign alias shows the others being placed.
#   6. A call that fails as a whole (the lease state lock held past its 30 seconds): one
#      `place failed reason=<message>` line, the warning `agentspace: The pending leases of '<project>' could
#      not be placed: <reason>; the next run places them.`, the host left `ready`, exit 0.
#
# NOT SHOWN here: "one lease per hold of the lease state lock". operator.md's "Which checks show it" gives
# it to the driver crate's test, tests\pending_place_pending.rs of agentspace-driver-ec2, in the row for
# place_pending taking the lease state lock once per lease. place_pending starts no program, so no stand-in is in play while it holds the
# lock, and a check from outside sees only the files it leaves. The cases above show what a hold per lease
# gives (a failure costing one lease, a lease released between holds never placed).
#
# A lease that place_pending leaves `pending` while the call succeeds is logged and not warned: one
# `place failed lease=<id> reason=<message>` line each, and no `could not be placed` warning; only a
# failure of the call as a whole warns, with the failure message without its `agentspace: ` prefix and
# its last period as the warning's <reason> (operator.md, provision, step 9). The cases assert that.
#
# SAFETY. See test-unmet-host-provision.ps1. The operator copy is written under the home's root, and a
# case that runs it points AGENTSPACE_OPERATOR_CMD at it for its own process and restores the value.
# The CA is the harness's throwaway one: no lease is signed with a real CA.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-provision-place.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$sec = 'operator.md#provision'
$drv = 'driver.md#place_pending'
$id1 = '11111111-1111-4111-8111-111111111111'
$id2 = '22222222-2222-4222-8222-222222222222'
$idOther = '33333333-3333-4333-8333-333333333333'
$idReleased = '44444444-4444-4444-8444-444444444444'
$ttl = 3600
$startSteps = 'run begun,deploy done,start done,sync done,bootstrap done,record done'
function Get-Alias([string]$Id, [string]$Project = 'agentspace') { "agentspace-$Project-$($Id.Substring(0, 8))" }
function Get-LeaseDir($H, [string]$Id) { Join-Path $H.UserProfile ".agentspace\leases\$Id" }

# The lease records the cases start from.
function Get-SeedLeases([string[]]$Ids = @($id1, $id2), [string]$Project = 'agentspace') {
  @(foreach ($i in $Ids) { New-HpLeaseRecord -Id $i -Project $Project }) + @((New-HpLeaseRecord -Id $idOther -Project 'other'), (New-HpLeaseRecord -Id $idReleased -Project $Project -State 'released'))
}
# A record as one comparable text: keys sorted, values as JSON.
function Format-Canon($V) {
  if ($null -eq $V) { return 'null' }
  if ($V -is [System.Collections.IDictionary]) { return '{' + ((@($V.Keys | Sort-Object { [string]$_ } -CaseSensitive) | ForEach-Object { "`"$_`":" + (Format-Canon $V[$_]) }) -join ',') + '}' }
  if ($V -is [System.Collections.IEnumerable] -and $V -isnot [string]) { return '[' + ((@($V) | ForEach-Object { Format-Canon $_ }) -join ',') + ']' }
  ConvertTo-Json -InputObject $V -Compress
}
function Get-LeaseRecord($H, [string]$Id) {
  $all = Read-HpLeases $H
  if ($null -eq $all) { return $null }
  foreach ($r in $all) { if ($r['id'] -ceq $Id) { return $r } }
  $null
}
# A placed lease's record: ready, expires_at a TTL after the placement, ssh, and nothing else changed.
function Add-PlacedProblems($Problems, $H, [string]$Id, $Seed, [DateTime]$T0, [DateTime]$T1, [string]$Project = 'agentspace') {
  $r = Get-LeaseRecord $H $Id
  if ($null -eq $r) { $Problems.Add("The record of $($Id.Substring(0, 8)) is gone"); return }
  if ($r['state'] -cne 'ready') { $Problems.Add("$($Id.Substring(0, 8)) is '$($r['state'])', not ready"); return }
  if (-not $r.ContainsKey('expires_at') -or -not (Test-HpTimeBetween $r['expires_at'] $T0.AddSeconds($ttl) $T1.AddSeconds($ttl))) { $Problems.Add("$($Id.Substring(0, 8)): expires_at is not a TTL after the placement") }
  if ($null -eq $r['ssh'] -or $r['ssh']['host_alias'] -cne (Get-Alias $Id $Project) -or $r['ssh']['user'] -cne 'ubuntu' -or @($r['ssh'].Keys).Count -ne 2) { $Problems.Add("$($Id.Substring(0, 8)): ssh is not {host_alias, user ubuntu}") }
  foreach ($k in 'id', 'project', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'driver') { if ((Format-Canon $r[$k]) -cne (Format-Canon $Seed[$k])) { $Problems.Add("$($Id.Substring(0, 8)): $k changed"); break } }
  $extra = @($r.Keys | Where-Object { $_ -notin 'id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'driver', 'expires_at', 'ssh' })
  if ($extra.Count -gt 0) { $Problems.Add("$($Id.Substring(0, 8)) holds a member that placement does not add") }
  foreach ($f in 'id_ed25519', 'id_ed25519.pub', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path (Get-LeaseDir $H $Id) $f) -PathType Leaf)) { $Problems.Add("$($Id.Substring(0, 8)): leases\<id>\$f was not made") } }
  $af = Get-HpAliasFile $H (Get-Alias $Id $Project)
  if (-not (Test-Path -LiteralPath $af -PathType Leaf)) { $Problems.Add("$($Id.Substring(0, 8)): the alias file was not written") }
}
# A lease left as it was: its record, and no files.
function Add-UnplacedProblems($Problems, $H, [string]$Id, $Seed, [string]$What, [string]$Project = 'agentspace') {
  $r = Get-LeaseRecord $H $Id
  if ($null -eq $r -or (Format-Canon $r) -cne (Format-Canon $Seed)) { $Problems.Add("$What's record changed") }
  if (Test-Path -LiteralPath (Join-Path (Get-LeaseDir $H $Id) 'id_ed25519')) { $Problems.Add("$What has a key") }
  if (Test-Path -LiteralPath (Get-HpAliasFile $H (Get-Alias $Id $Project))) { $Problems.Add("$What has an alias file") }
}
function Read-AliasLines($H, [string]$Id, [string]$Project = 'agentspace') {
  $af = Get-HpAliasFile $H (Get-Alias $Id $Project)
  if (-not (Test-Path -LiteralPath $af -PathType Leaf)) { return @() }
  @([IO.File]::ReadAllText($af) -split "`n" | Where-Object { $_ -ne '' })
}
function Get-SeedHome($H, $S, [string]$Seed = 'creating', $Leases = (Get-SeedLeases)) {
  $rec = if ($Seed -eq 'ready') { New-HpRecord 'agentspace' 'ready' $script:HpHostId '2026-01-01T00:00:00Z' } else { New-HpRecord 'agentspace' 'creating' '' '2026-01-01T00:00:00Z' }
  Initialize-HpHome -H $H -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @($rec))
  Set-HpLeases $H $Leases
  $aws = Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists:($Seed -eq 'ready')
  Set-HpScenario -H $H -S $S -LockProject 'agentspace' -Scenario @{ aws = $aws; rules = @((Get-HpBootstrapRule)) }
}
$distDir = Split-Path -Parent (Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD')
# A copy of an installed program is never made: the cases that copy the operator report SKIPPED when the
# operator under test is the one in the real profile's .agentspace folder (no AGENTSPACE_OPERATOR_CMD
# set), since AGENTS.md ("Terry's real files in a review") keeps a check from reading that folder.
$realAgentspace = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.agentspace'
$operatorIsInstalled = [IO.Path]::GetFullPath([string](Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD')).StartsWith($realAgentspace + '\', [StringComparison]::OrdinalIgnoreCase)
$installedReason = 'The operator under test is the installed one, which this check does not copy; set AGENTSPACE_OPERATOR_CMD to a build'

# ---- 1. A host made by the run, its pending leases placed --------------------------------------------------
Invoke-HpInHome -Name "${drv}: a host made by the run" -Body {
  param($h, $s)
  $seeds = Get-SeedLeases
  Get-SeedHome $h $s 'creating' $seeds
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $t1 = [DateTime]::UtcNow
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr
  $byId = @{}; foreach ($x in $seeds) { $byId[$x['id']] = $x }

  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=2,run done") { $p.Add("The log's lines are not: $startSteps, place done placed=2, run done") }
  if ($lines | Where-Object { $_ -like '*could not be placed*' }) { $p.Add('The warning was printed though every lease was placed') }
  Write-HpProblems "${sec}: place_pending is called after the host is recorded ready and logs place done placed=<n>; with every lease placed there is no warning, and exit 0 (step 9)" $p

  $p = New-HpProblems
  foreach ($i in $id1, $id2) { Add-PlacedProblems $p $h $i $byId[$i] $t0 $t1 }
  $all = Read-HpLeases $h
  if ($null -eq $all -or (@($all | ForEach-Object { $_['id'] }) -join ',') -cne (@($seeds | ForEach-Object { $_['id'] }) -join ',')) { $p.Add('The order or the set of records changed') }
  Write-HpProblems "${drv}: each pending lease of the project is placed: state ready, expires_at a TTL after its own placement, ssh the alias and ubuntu, the key, certificate and alias written, no other member changed (steps 2.5 and 2.6)" $p

  $p = New-HpProblems
  if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=2,run done") { $p.Add('The run did not place the project''s leases, so nothing shows the others were left alone') }
  Add-UnplacedProblems $p $h $idOther $byId[$idOther] 'another project''s pending lease'
  Add-UnplacedProblems $p $h $idReleased $byId[$idReleased] 'a released lease'
  Write-HpProblems "${drv}: every other record is left as it was: another project's pending lease and a released one, with no files made for them (what it does)" $p

  $p = New-HpProblems
  foreach ($i in $id1, $id2) {
    $a = Get-Alias $i
    $al = Read-AliasLines $h $i
    if ($al.Count -lt 5) { $p.Add("$($i.Substring(0, 8)): the alias file is missing or short"); continue }
    if (-not $al[0].StartsWith("# agentspace lease $i;")) { $p.Add("$($i.Substring(0, 8)): the first line is not the lease's owner mark") }
    if ($al[1] -cne "Host $a") { $p.Add("$($i.Substring(0, 8)): the Host line is not Host <alias>") }
    $hk = [Array]::IndexOf($al, '  HostKeyAlias agentspace-host.agentspace')
    if ($hk -lt 0) { $p.Add("$($i.Substring(0, 8)): no HostKeyAlias agentspace-host.<project>: the alias is not the form for a host that is not the box") }
    elseif ($al[$hk + 1] -notmatch '^  UserKnownHostsFile "[^"]+"$') { $p.Add("$($i.Substring(0, 8)): no UserKnownHostsFile line straight after HostKeyAlias") }
    if ($al -cnotcontains '  StrictHostKeyChecking yes') { $p.Add("$($i.Substring(0, 8)): StrictHostKeyChecking is not yes") }
    $pc = @($al | Where-Object { $_ -like '  ProxyCommand *' })
    $want = '  ProxyCommand "' + (Join-Path $distDir 'agentspace-ssh-proxy.exe') + '" ' + $a
    if ($pc.Count -ne 1 -or -not ($pc[0] -ieq $want)) { $p.Add("$($i.Substring(0, 8)): the ProxyCommand line is not the program beside the run's own executable, then the alias") }
  }
  $cfg = Join-Path $h.SshDir 'config'
  if (-not (Test-Path -LiteralPath $cfg -PathType Leaf) -or ([IO.File]::ReadAllText($cfg) -notmatch '(?m)^Include agentspace\.d/\*\r?$')) { $p.Add('The Include line is not in the ssh folder''s config') }
  Write-HpProblems "${drv}: a placed lease's alias is the form for a host that is not the box, with the default ProxyCommand line naming agentspace-ssh-proxy.exe beside the run's own executable, and the Include line is added (D8, D47; state.md, The alias file)" $p
}

# ---- 2. A killed placement's leftovers ----------------------------------------------------------------------
Invoke-HpInHome -Name "${drv}: a killed placement's leftover files" -Body {
  param($h, $s)
  $seeds = Get-SeedLeases
  Get-SeedHome $h $s 'ready' $seeds
  $dir = Get-LeaseDir $h $id1
  Write-HpFile -H $h -Path (Join-Path $dir 'id_ed25519') -Text 'a key a killed placement left'
  Write-HpFile -H $h -Path (Join-Path $dir 'id_ed25519.pub') -Text 'a public key a killed placement left'
  Write-HpFile -H $h -Path (Join-Path $dir 'id_ed25519-cert.pub') -Text 'a certificate a killed placement left'
  $aliasText = "# agentspace lease $id1; written by a killed placement`nHost $(Get-Alias $id1)`n"
  Write-HpFile -H $h -Path (Get-HpAliasFile $h (Get-Alias $id1)) -Text $aliasText
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $t1 = [DateTime]::UtcNow
  $log = Read-HpLog $h 'agentspace'
  $byId = @{}; foreach ($x in $seeds) { $byId[$x['id']] = $x }
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  foreach ($i in $id1, $id2) { Add-PlacedProblems $p $h $i $byId[$i] $t0 $t1 }
  if ((Get-HpFileBytes (Join-Path $dir 'id_ed25519')) -and [Text.Encoding]::UTF8.GetString((Get-HpFileBytes (Join-Path $dir 'id_ed25519'))) -match 'a key a killed placement left') { $p.Add('The killed placement''s key is still there') }
  if ([Text.Encoding]::UTF8.GetString((Get-HpFileBytes (Join-Path $dir 'id_ed25519-cert.pub'))) -match 'a certificate a killed placement left') { $p.Add('The killed placement''s certificate is still there') }
  $al = Read-AliasLines $h $id1
  if ($al.Count -lt 5) { $p.Add('The lease''s alias file is the killed placement''s, not a new one') }
  if ((Get-HpLogShape $log) -cne "run begun,deploy done,start done,sync done,bootstrap done,record done,place done placed=2,run done") { $p.Add('The log does not show both leases placed and no failure') }
  Write-HpProblems "${drv}: a killed placement's leftover key, certificate and alias file are removed first, and the lease is placed (step 2.4; state.md, The alias file)" $p
}

# ---- 3. A lease whose alias is a foreign file -----------------------------------------------------------------
Invoke-HpInHome -Name "${drv}: a foreign file at one lease's alias" -Body {
  param($h, $s)
  $seeds = Get-SeedLeases
  Get-SeedHome $h $s 'ready' $seeds
  $foreign = "# a person's own file`nHost mine`n"
  $fpath = Get-HpAliasFile $h (Get-Alias $id2)
  Write-HpFile -H $h -Path $fpath -Text $foreign
  $t0 = [DateTime]::UtcNow
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $t1 = [DateTime]::UtcNow
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr
  $byId = @{}; foreach ($x in $seeds) { $byId[$x['id']] = $x }
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  Add-PlacedProblems $p $h $id1 $byId[$id1] $t0 $t1
  $rec2 = Get-LeaseRecord $h $id2
  if ($null -eq $rec2 -or (Format-Canon $rec2) -cne (Format-Canon $byId[$id2])) { $p.Add('The lease with a foreign alias file was changed: it MUST stay pending') }
  if (Test-Path -LiteralPath (Join-Path (Get-LeaseDir $h $id2) 'id_ed25519')) { $p.Add('A key was left for the lease that could not be placed') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes $fpath) ([Text.UTF8Encoding]::new($false).GetBytes($foreign)))) { $p.Add('The foreign file was changed or removed') }
  if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=1,place failed,run done") { $p.Add("The log's lines are not: $startSteps, place done placed=1, place failed, run done") }
  else {
    $pf = $log.Lines | Where-Object { $_.Step -eq 'place' -and $_.Outcome -eq 'failed' }
    if ($pf.Rest -notmatch "^ lease=$id2 reason=\S.*$") { $p.Add('The failed line is not place failed lease=<id> reason=<message>') }
  }
  if ($lines | Where-Object { $_ -like '*could not be placed*' }) { $p.Add('A warning was printed for a lease that failed inside a call that succeeded: step 9 logs it and does not warn') }
  Write-HpProblems "${drv}: a lease that cannot be placed (a foreign file at its alias) stays pending with place failed lease=<id> reason=<message> logged after place done placed=<n>, the foreign file kept, the others placed, exit 0 (step 2.7; its log)" $p
}

# ---- 4. A lease released while the run works -------------------------------------------------------------------
Invoke-HpInHome -Name "${drv}: a lease released meanwhile" -Body {
  param($h, $s)
  $seeds = Get-SeedLeases
  Get-SeedHome $h $s 'ready' $seeds
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists); rules = @(@{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; hook = 'boot' }) }
  $job = Start-HpOperatorJob -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $reached = Wait-HpHook -S $s -Name 'boot' -Job $job
  $releasedRecord = New-HpLeaseRecord -Id $id2 -State 'released'
  if ($reached) {
    # Between the run's start and its place_pending, the caller releases the second lease.
    Set-HpLeases $h @((New-HpLeaseRecord -Id $id1), $releasedRecord, (New-HpLeaseRecord -Id $idOther -Project 'other'), (New-HpLeaseRecord -Id $idReleased -State 'released'))
    Release-HpHook -H $h -S $s -Name 'boot'
  }
  $t0 = [DateTime]::UtcNow.AddMinutes(-5)
  $r = Wait-HpOperatorJob $job
  $t1 = [DateTime]::UtcNow
  $log = Read-HpLog $h 'agentspace'
  $p = New-HpProblems
  if (-not $reached) { $p.Add('The run never reached bootstrap') }
  Add-HpRunProblems $p $r 0
  Add-PlacedProblems $p $h $id1 $seeds[0] $t0 $t1
  Add-UnplacedProblems $p $h $id2 $releasedRecord 'the lease released meanwhile'
  if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=1,run done") { $p.Add('The log is not place done placed=1 with no failed line: a lease released meanwhile is skipped, not failed') }
  Write-HpProblems "${drv}: a lease released after the run began is never placed: its record is not changed, no file is made for it, and it is no failure (step 2.2: the file is read again for each lease)" $p
}

# ---- 5. The operator run from a folder without the ProxyCommand program ----------------------------------------
# A host that is not the box needs agentspace-ssh-proxy.exe beside the operator for its own alias (step 4 of
# "How the operator reaches a host", reached in provision's step 6), so a run for it fails before step 9 from
# such a folder (operator.md, provision, step 9): the box's run is the one that reaches step 9, and its lease
# aliases name the same program (How each program finds the others).
$boxProject = 'swimcoach'
function Get-BoxHome($H, $S) {
  Initialize-HpHome -H $H
  $leases = Get-SeedLeases -Project $boxProject
  Set-HpLeases $H $leases
  $aws = @{ 'cloudformation describe-stacks' = @((New-HpStack 'agentbox' 'UPDATE_COMPLETE' $script:HpBoxId)); 'cloudformation deploy' = @((New-HpStep "deploy-out`n")); 'ec2 describe-instances' = @((New-HpDescribe)) }
  Set-HpScenario -H $H -S $S -LockProject $boxProject -Scenario @{ aws = $aws; rules = @((Get-HpBootstrapRule)) }
  $leases
}
function Test-FromCopy([string]$Name, [hashtable]$Extra, [string]$Kind) {
  if ($operatorIsInstalled) { Write-ConformanceResult -Name $Name -Status SKIPPED -Reason $installedReason; return }
  Invoke-HpInHome -Name $Name -Body {
    param($h, $s)
    $seeds = Get-BoxHome $h $s
    $folder = Join-Path $h.Root 'opcopy'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $folder) | Out-Null
    $copy = Join-Path $folder 'agentspace-operator.exe'
    Copy-Item -LiteralPath (Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD') -Destination (Assert-ConformancePath -ConformanceHome $h -Path $copy)
    if ($Kind -eq 'cmd') { Write-HpFile -H $h -Path (Join-Path $folder 'agentspace-ssh-proxy.cmd') -Text "@exit /b 1`r`n" }
    $saved = [Environment]::GetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD')
    $t0 = [DateTime]::UtcNow
    try {
      [Environment]::SetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD', $copy)
      $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', $boxProject) -Extra $Extra
    } finally { [Environment]::SetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD', $saved) }
    $t1 = [DateTime]::UtcNow
    $log = Read-HpLog $h $boxProject
    $lines = Get-HpLines $r.Stderr
    $byId = @{}; foreach ($x in $seeds) { $byId[$x['id']] = $x }
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if ($null -eq (Get-HpRecord $h $boxProject) -or (Get-HpRecord $h $boxProject)['state'] -cne 'ready') { $p.Add('The host is not left ready') }
    if ($Kind -eq 'missing') {
      foreach ($i in $id1, $id2) { Add-UnplacedProblems $p $h $i $byId[$i] "lease $($i.Substring(0, 8))" $boxProject }
      $shape = Get-HpLogShape $log
      if ($shape -cne "$startSteps,place done placed=0,place failed,place failed,run done") { $p.Add("The log's lines are not: $startSteps, place done placed=0, then one place failed line for each lease, run done") }
      else {
        $pf = @($log.Lines | Where-Object { $_.Step -eq 'place' -and $_.Outcome -eq 'failed' })
        if ($pf[0].Rest -notmatch "^ lease=$id1 reason=\S.*$" -or $pf[1].Rest -notmatch "^ lease=$id2 reason=\S.*$") { $p.Add('The failed lines are not place failed lease=<id> reason=<message>, in the order the leases were tried') }
      }
      if ($lines | Where-Object { $_ -like '*could not be placed*' }) { $p.Add('A warning was printed for leases that failed inside a call that succeeded') }
    } else {
      foreach ($i in $id1, $id2) { Add-PlacedProblems $p $h $i $byId[$i] $t0 $t1 $boxProject }
      if ((Get-HpLogShape $log) -cne "$startSteps,place done placed=2,run done") { $p.Add('The log does not show both leases placed') }
      foreach ($i in $id1, $id2) {
        $pc = @(Read-AliasLines $h $i $boxProject | Where-Object { $_ -like '  ProxyCommand *' })
        $a = Get-Alias $i $boxProject
        $want = if ($Kind -eq 'override') { '  ProxyCommand ' + $Extra['AGENTSPACE_PROXY_COMMAND'].Replace('%', '%%') }
        else { '  ProxyCommand ' + (Join-Path $env:SystemRoot 'System32\cmd.exe') + ' /d /c "' + (Join-Path $folder 'agentspace-ssh-proxy.cmd') + '" ' + $a }
        if ($pc.Count -ne 1 -or -not ($pc[0] -ieq $want)) { $p.Add("$($i.Substring(0, 8)): the ProxyCommand line is not the $(if ($Kind -eq 'override') { 'override, as it was set' } else { 'interpreter form with the .cmd beside the operator' })") }
      }
    }
    Write-HpProblems $Name $p
  }
}
Test-FromCopy "${drv}: a ProxyCommand program missing beside the operator, for the box's run: every lease of the project stays pending, each with its place failed line, the host stays ready" @{} 'missing'
Test-FromCopy "${drv}: AGENTSPACE_PROXY_COMMAND replaces the whole line, so the program missing beside the operator is not looked for and the leases are placed" @{ AGENTSPACE_PROXY_COMMAND = "$env:ComSpec /d /c exit 1" } 'override'
Test-FromCopy "${drv}: with no .exe but an agentspace-ssh-proxy.cmd beside the operator the leases are placed with the interpreter form of the line (How each program finds the others)" @{} 'cmd'

$copyCaseName = "${sec}: a run for a host that is not the box, from a folder without agentspace-ssh-proxy.exe"
if ($operatorIsInstalled) { Write-ConformanceResult -Name $copyCaseName -Status SKIPPED -Reason $installedReason }
else { Invoke-HpInHome -Name $copyCaseName -Body {
  param($h, $s)
  Initialize-HpHome -H $h -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'creating' '' '2026-01-01T00:00:00Z')))
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId); rules = @((Get-HpBootstrapRule)) }
  $folder = Join-Path $h.Root 'opcopy'
  New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $folder) | Out-Null
  $copy = Join-Path $folder 'agentspace-operator.exe'
  Copy-Item -LiteralPath (Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD') -Destination (Assert-ConformancePath -ConformanceHome $h -Path $copy)
  $saved = [Environment]::GetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD')
  try {
    [Environment]::SetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD', $copy)
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  } finally { [Environment]::SetEnvironmentVariable('AGENTSPACE_OPERATOR_CMD', $saved) }
  $calls = @(Get-HpCalls $s)
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  $m = if ($lines.Count -gt 0) { [regex]::Match($lines[-1], '^agentspace: (.+) is missing; the operator''s alias for a host needs it beside the operator\.$') } else { $null }
  if ($null -eq $m -or -not $m.Success) { $p.Add('The last line of stderr is not agentspace: <path> is missing; the operator''s alias for a host needs it beside the operator.') }
  elseif ($m.Groups[1].Value -notmatch 'agentspace-ssh-proxy\.exe$') { $p.Add('The path in the message is not agentspace-ssh-proxy.exe') }
  if (@($calls | Where-Object { $_['kind'] -in 'ssh', 'scp' }).Count -gt 0) { $p.Add('An ssh or scp was started: the alias is refused before the first connection') }
  if ((Get-HpLogShape $log) -cne 'run begun,deploy done,start done,sync failed') { $p.Add('The log is not run begun, deploy done, start done, sync failed') }
  $rec = Get-HpRecord $h 'agentspace'
  if ($null -eq $rec -or $rec['state'] -cne 'failed') { $p.Add('The record of a host that was never ready is not failed') }
  Write-HpProblems "${sec}: for a host that is not the box, a run from a folder without agentspace-ssh-proxy.exe fails at sync with the alias's message, starts no ssh, logs sync failed and records failed (step 6; How the operator reaches a host)" $p
} }

# ---- 6. The call fails as a whole -------------------------------------------------------------------------------
Invoke-HpInHome -Name "${sec}: a place_pending that fails as a whole" -Body {
  param($h, $s)
  $seeds = Get-SeedLeases
  Get-SeedHome $h $s 'ready' $seeds
  Set-HpScenario -H $h -S $s -LockProject 'agentspace' -Scenario @{ aws = (Get-HpHostAws -Project 'agentspace' -Id $script:HpHostId -Exists); rules = @(@{ kind = 'ssh'; class = 'bootstrap'; hook = 'boot' }) }
  $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
  $job = Start-HpOperatorJob -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
  $reached = Wait-HpHook -S $s -Name 'boot' -Job $job
  $held = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  try {
    if ($reached) {
      # The lease state lock held past its 30 seconds: place_pending cannot take it, so the call fails.
      $held = Hold-HpLock -H $h -Path (Join-Path $h.StateDir 'leases.lock')
      Release-HpHook -H $h -S $s -Name 'boot'
    }
    $r = Wait-HpOperatorJob $job
  } finally { if ($held) { $held.Dispose() } }
  $clock.Stop()
  $log = Read-HpLog $h 'agentspace'
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  if (-not $reached) { $p.Add('The run never reached bootstrap') }
  Add-HpRunProblems $p $r 0
  if ((Get-HpLogShape $log) -cne "$startSteps,place failed,run done") { $p.Add("The log's lines are not: $startSteps, one place failed line, run done") }
  else {
    $pf = $log.Lines | Where-Object { $_.Step -eq 'place' }
    if ($pf.Rest -notmatch '^ reason=\S.*$' -or $pf.Rest -match 'lease=') { $p.Add('The line is not place failed reason=<message> with no lease=') }
    else {
      $reason = $pf.Rest.Substring(' reason='.Length)
      # The log's reason is the failure message without its `agentspace: ` prefix ("Its log"); the warning's
      # <reason> is that without its last period as well (operator.md, provision, step 9).
      $wantReason = if ($reason.EndsWith('.')) { $reason.Substring(0, $reason.Length - 1) } else { $reason }
      $m = if ($lines.Count -gt 0) { [regex]::Match($lines[-1], '^agentspace: The pending leases of ''agentspace'' could not be placed: (.+); the next run places them\.$') } else { $null }
      if ($null -eq $m -or -not $m.Success) { $p.Add('The last line of stderr is not agentspace: The pending leases of ''<project>'' could not be placed: <reason>; the next run places them.') }
      elseif ($m.Groups[1].Value -cne $wantReason) { $p.Add('The warning''s reason is not the failure message without its agentspace: prefix and its last period') }
    }
  }
  if ($clock.Elapsed.TotalSeconds -lt 25) { $p.Add('It gave up before the lease state lock''s 30 seconds') }
  $rec = Get-HpRecord $h 'agentspace'
  if ($null -eq $rec -or $rec['state'] -cne 'ready') { $p.Add('The host is not left ready') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $h)) $leasesBefore)) { $p.Add('The lease state file changed') }
  Write-HpProblems "${sec}: a place_pending that fails as a whole (the lease state lock held past its 30 seconds) logs one place failed reason=<message>, warns, leaves the host ready and the leases pending, and exits 0 (step 9)" $p
}

Exit-ConformanceRun

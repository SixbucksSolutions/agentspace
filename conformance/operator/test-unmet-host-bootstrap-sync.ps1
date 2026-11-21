#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `bootstrap` and of `sync -Host` (docs/operator.md, "bootstrap",
# "sync", "How the operator reaches a host", "The operator's folder, one per command", "Which checks
# show it"), run through the test seam. Card #0156 (E0.07-T04-S02). `sync -All` is in
# test-unmet-host-bootstrap-sync-all.ps1.
#
# EXPECTED TO FAIL AGAINST v0.06's OPERATOR, which has no `bootstrap` and no `-Host`: every case reports a
# FAIL there. A v0.07 build MUST pass every result line.
#
# What it does: each case makes its own throwaway home (the harness's, with a throwaway CA) holding a
# host registry with the box and one other host, both with an instance ID, and the stand-ins of
# conformance/operator/host-provision-common.ps1: an `aws` that reports a running instance launched long
# ago, an `ssh` that records its arguments and its standard input, an `scp` and a `go` that record
# their arguments (and, for each local path, what it held at the moment of the call). `go` is found
# through a PATH of the check's own, so no case starts the real `go`. sync's step 7 builds only when
# C:\Projects\localswim exists (operator.md, sync, step 7), and the `go` stand-in runs with that folder as its
# working folder, so a case that shows the build MUST report SKIPPED, naming the folder, when the folder
# is absent. That is one Test-Path -PathType Container on it; no case lists, reads or writes anything in it.
#
#   1. `bootstrap -Host <project>`: the one ssh's arguments (`-o ClearAllForwardings=yes <alias>
#      <remote command>`, no BatchMode), the script's bytes on its stdin equal to the repository's
#      box/bootstrap.sh, its stdout and stderr passed through, the progress lines, `Bootstrapped: <id>`,
#      its failure message for any non-zero exit (255 too), `-Quiet`, and the CA checked first so that a
#      missing CA fails before any ssh for a host that is not the box and never matters for the box.
#   2. `bootstrap` and `bootstrap -Host swimcoach` for the box: the alias `agentspace`, the box's
#      instance ID, no key, certificate or alias written.
#   3. `sync -Host <project>` with a CLAUDE.md, a localswim receipt and a `go`: every ssh and scp, in the
#      order of operator.md's table, names the host's alias with exactly the arguments the table gives,
#      the system folder and the build in one operator\<hex>\ folder that the stand-ins see, the system
#      folder's files the repository's own, and the folder removed at the end.
#   4. `sync` and `sync -Host swimcoach` for the box: the same calls through the alias `agentspace`.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under the
# same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and
# reports SKIPPED, running nothing, if it does not. A failure reason never quotes the program's output.
# `ssh-keygen` has no seam (operator.md, Q30): the real `ssh-keygen.exe` makes the operator's key and
# certificate against the throwaway CA. What the seam cannot reach, a real key exchange, a real `go build`
# and sshd's trust of the CA, is for E0.07-T05-S01's live check.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-bootstrap-sync.ps1
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$bs = 'operator.md#bootstrap'
$sy = 'operator.md#sync'
$alias = 'agentspace-host.agentspace'
$caf = 'ClearAllForwardings=yes'
$staging = '/tmp/agentspace-system'
$bootErr = 'The toolchain bootstrap failed on the instance.'

function Initialize-Setup($H, $S, [hashtable]$Rule = @{}) {
  Initialize-HpHome -H $H -HostsText (Get-HpHostsText -BoxId $script:HpBoxId -Records @((New-HpRecord 'agentspace' 'ready' $script:HpHostId '2026-01-01T00:00:00Z')))
  $rules = @((@{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; err = "bootstrap-err`n" } + $Rule))
  Set-HpScenario -H $H -S $S -Scenario @{ aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) }; rules = $rules }
}
# The remote command of an ssh or scp-less call: its last argument, under the rule that it holds no
# percent sign, no double quote and no control character and does not end in a backslash.
function Add-RemoteCommandProblems($Problems, $Args1, [string]$Which) {
  $cmd = [string]$Args1[$Args1.Count - 1]
  if ($cmd.Length -eq 0) { $Problems.Add("$Which's remote command is empty") }
  if ($cmd -match '[%"\x00-\x1f\x7f]' -or $cmd.EndsWith('\')) { $Problems.Add("$Which's remote command holds a percent sign, a double quote or a control character, or ends in a backslash") }
}
function Test-Same([string[]]$A, [string[]]$B) { $A.Count -eq $B.Count -and (($A -join [char]0) -ceq ($B -join [char]0)) }

# ---- 1. bootstrap -Host <project> -----------------------------------------------------------------------------
Invoke-HpInHome -Name "${bs}: bootstrap -Host" -Body {
  param($h, $s)
  Initialize-Setup $h $s
  $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap', '-Host', 'agentspace')
  $calls = @(Get-HpCalls $s)
  $ssh = @($calls | Where-Object { $_['kind'] -eq 'ssh' })
  $lines = Get-HpLines $r.Stderr

  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($r.Stdout -cne "bootstrap-out`n") { $p.Add('Stdout is not the bootstrap''s own stdout') }
  if ($lines -cnotcontains 'bootstrap-err') { $p.Add('The bootstrap''s own stderr did not pass through') }
  $iReady = [Array]::IndexOf($lines, "Ready: $($script:HpHostId)")
  $iRun = [Array]::IndexOf($lines, 'Running the toolchain bootstrap on the instance...')
  $iErr = [Array]::IndexOf($lines, 'bootstrap-err')
  $iDone = [Array]::IndexOf($lines, "Bootstrapped: $($script:HpHostId)")
  if (-not (0 -le $iReady -and $iReady -lt $iRun -and $iRun -lt $iErr -and $iErr -lt $iDone)) { $p.Add('stderr does not hold Ready: <id>, Running the toolchain bootstrap on the instance..., the script''s stderr and Bootstrapped: <id>, in that order') }
  Write-HpProblems "${bs}: bootstrap -Host starts the host, runs the script and prints Bootstrapped: <id>: exit 0, the bootstrap's stdout on stdout, its stderr passed through, the progress lines in order (steps 1, 3 and 5)" $p

  $p = New-HpProblems
  if ($ssh.Count -ne 1) { $p.Add("ssh was started $($ssh.Count) times, not once") }
  if (@($calls | Where-Object { $_['kind'] -in 'scp', 'go' }).Count -gt 0) { $p.Add('A scp or go was started: bootstrap copies and builds nothing') }
  if ($ssh.Count -ge 1) {
    $a = @($ssh[0]['a'])
    if ($a.Count -ne 4 -or $a[0] -cne '-o' -or $a[1] -cne $caf -or $a[2] -cne $alias) { $p.Add('The ssh''s arguments are not -o ClearAllForwardings=yes <alias> <remote command>, the alias agentspace-host.<project>, and no BatchMode') }
    Add-RemoteCommandProblems $p $a 'bootstrap'
  }
  Write-HpProblems "${bs}: the ssh's arguments are -o ClearAllForwardings=yes agentspace-host.<project> and one remote command, its last, with no BatchMode (step 3; The test seam)" $p

  $p = New-HpProblems
  if ($ssh.Count -ge 1) {
    $got = Get-HpStdin $s $ssh[0]['n']
    if (-not (Test-HpSameBytes $got (Get-HpBootstrapBytes))) { $p.Add('The bytes on the ssh''s stdin are not exactly box/bootstrap.sh''s') }
  } else { $p.Add('No ssh was started') }
  Write-HpProblems "${bs}: the ssh's stdin carries the repository's box/bootstrap.sh byte for byte, then is closed, so the stand-in reads to its end (step 3)" $p

  $p = New-HpProblems
  $aws = @($calls | Where-Object { $_['kind'] -eq 'aws' })
  if (@($aws | Where-Object { (Get-HpCallName $_) -ne 'aws ec2 describe-instances' }).Count -gt 0) { $p.Add('bootstrap made an AWS call other than the start and wait''s') }
  if (@($aws | Where-Object { $_['a'] -notcontains $script:HpHostId }).Count -gt 0) { $p.Add('An AWS call does not name the host''s instance') }
  if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $h)) $hostsBefore)) { $p.Add('The registry was written: bootstrap writes no record') }
  foreach ($f in 'id_ed25519', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'agentspace') $f) -PathType Leaf)) { $p.Add("hosts\agentspace\$f was not made") } }
  if (-not (Test-Path -LiteralPath (Get-HpAliasFile $h $alias) -PathType Leaf)) { $p.Add('The host''s operator alias was not written') }
  Write-HpProblems "${bs}: bootstrap starts the host with its own instance ID and nothing else from AWS, writes no record, and makes the operator's key, certificate and alias for a host that is not the box (steps 1 and 2)" $p

  foreach ($code in 7, 255) {
    $c2 = $code
    Set-HpScenario -H $h -S $s -Scenario @{ aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) }; rules = @(@{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; code = $c2 }) }
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap', '-Host', 'agentspace')
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne "agentspace: $bootErr") { $p.Add('The last line of stderr is not agentspace: The toolchain bootstrap failed on the instance.') }
    if ($lines | Where-Object { $_ -like 'Bootstrapped:*' }) { $p.Add('Bootstrapped was printed') }
    if ($r.Stdout -cne "bootstrap-out`n") { $p.Add('The bootstrap''s stdout did not pass through') }
    Write-HpProblems "${bs}: an ssh that exits $c2 fails bootstrap with agentspace: The toolchain bootstrap failed on the instance., exit 1, and no Bootstrapped line (step 4)" $p
  }

  Set-HpScenario -H $h -S $s -Scenario @{ aws = @{ 'ec2 describe-instances' = @((New-HpDescribe)) }; rules = @(@{ kind = 'ssh'; class = 'bootstrap'; out = "bootstrap-out`n"; err = "bootstrap-err`n" }) }
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap', '-Host', 'agentspace', '-Quiet')
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  if ($r.Stdout -cne "bootstrap-out`n") { $p.Add('-Quiet changed what passes through to stdout') }
  if ($r.Stderr -match 'Running the toolchain|Bootstrapped:|Ready:') { $p.Add('-Quiet left a progress message') }
  if ((Get-HpLines $r.Stderr) -cnotcontains 'bootstrap-err') { $p.Add('-Quiet suppressed the bootstrap''s own stderr') }
  Write-HpProblems "${bs}: -Quiet prints no progress line and still passes the bootstrap's output through" $p
}

# ---- 2. bootstrap for the box ------------------------------------------------------------------------------------
foreach ($args1 in @(@('bootstrap'), @('bootstrap', '-Host', 'swimcoach'))) {
  $a1 = $args1
  Invoke-HpInHome -Name "${bs}: $($a1 -join ' ') acts on the box" -Body {
    param($h, $s)
    Initialize-Setup $h $s
    $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
    $r = Invoke-HpOperator -H $h -S $s -Arguments $a1
    $calls = @(Get-HpCalls $s)
    $ssh = @($calls | Where-Object { $_['kind'] -eq 'ssh' })
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if ($ssh.Count -ne 1 -or -not (Test-Same @($ssh[0]['a'] | Select-Object -First 3) @('-o', $caf, 'agentspace')) -or @($ssh[0]['a']).Count -ne 4) { $p.Add('The ssh is not -o ClearAllForwardings=yes agentspace <remote command>') }
    elseif (-not (Test-HpSameBytes (Get-HpStdin $s $ssh[0]['n']) (Get-HpBootstrapBytes))) { $p.Add('The bytes on the ssh''s stdin are not exactly box/bootstrap.sh''s') }
    $aws = @($calls | Where-Object { $_['kind'] -eq 'aws' })
    if ($aws.Count -eq 0 -or @($aws | Where-Object { $_['a'] -notcontains $script:HpBoxId }).Count -gt 0) { $p.Add('The AWS calls do not name the box''s instance') }
    if ($r.Stderr -notmatch "(?m)^Bootstrapped: $($script:HpBoxId)$") { $p.Add('stderr lacks Bootstrapped: <the box''s id>') }
    if (Test-Path -LiteralPath (Join-Path $h.UserProfile '.agentspace\hosts\swimcoach\id_ed25519')) { $p.Add('A key was made for the box') }
    if (Test-Path -LiteralPath (Join-Path $h.SshDir 'agentspace.d')) { $p.Add('An alias was written for the box') }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $h)) $hostsBefore)) { $p.Add('The registry was written') }
    Write-HpProblems "${bs}: $($a1 -join ' ') runs the script through the alias agentspace on the box's instance, and writes no key, certificate, alias or record (steps 1 to 3; -Host swimcoach is the box)" $p
  }
}

# ---- 1b. The CA check comes first, and the box needs none ----------------------------------------------------------
Invoke-HpInHome -Name "${bs}: the CA is checked first for a host that is not the box" -Body {
  param($h, $s)
  Initialize-Setup $h $s
  Remove-HpPath -H $h -Path (Join-Path $h.UserProfile '.agentspace\ca')
  $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap', '-Host', 'agentspace')
  $calls = @(Get-HpCalls $s)
  $lines = Get-HpLines $r.Stderr
  $p = New-HpProblems
  Add-HpRunProblems $p $r 1
  if (@($calls | Where-Object { $_['kind'] -eq 'ssh' }).Count -ne 0) { $p.Add('An ssh was started with no CA') }
  if ($lines.Count -eq 0 -or $lines[-1] -notmatch '^agentspace: .+agentspace_ca does not exist; run ca\.$') { $p.Add('The last line of stderr is not the CA check''s own message: agentspace: <key> does not exist; run ca.') }
  if ($lines | Where-Object { $_ -like 'Running the toolchain*' }) { $p.Add('The bootstrap was announced before the CA was checked') }
  if (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'agentspace') 'id_ed25519')) { $p.Add('A key was made before the CA was checked') }
  Write-HpProblems "${bs}: for a host that is not the box the CA is checked before the operator's key is made, so a missing CA fails with ca's message before any ssh (step 2; How the operator reaches a host)" $p

  $r = Invoke-HpOperator -H $h -S $s -Arguments @('bootstrap')
  $p = New-HpProblems
  Add-HpRunProblems $p $r 0
  Write-HpProblems "${bs}: the box's bootstrap makes no operator key or certificate, so it needs no CA and succeeds without one (step 2: for a host that is not the box)" $p
}

# ---- 3. sync -Host <project> ----------------------------------------------------------------------------------------
function Test-SyncCalls([string]$Name, [string[]]$Arguments, [string]$Alias, [string]$Id, [bool]$IsBox) {
  Invoke-HpInHome -Name $Name -NeedsLocalswim -Body {
    param($h, $s)
    Initialize-Setup $h $s
    New-HpClaudeMd $h
    New-HpFakeLocalswim -H $h | Out-Null
    $hostsBefore = Get-HpFileBytes (Get-HpHostsFile $h)
    $r = Invoke-HpOperator -H $h -S $s -Arguments $Arguments -WithGo
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $ca = Join-Path $h.UserProfile '.agentspace\ca\agentspace_ca.pub'
    $claude = Join-Path $h.UserProfile '.claude\CLAUDE.md'

    $want = @('aws ec2 describe-instances', 'aws ec2 describe-instances', 'ssh:sync', 'scp', 'scp', 'ssh:sync', 'ssh:sync', 'scp', 'go', 'scp', 'ssh:sync', 'ssh:push')
    $got = Get-HpSequence $calls
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if (-not (Test-Same $got $want)) {
      $i = 0
      while ($i -lt $got.Count -and $i -lt $want.Count -and $got[$i] -ceq $want[$i]) { $i++ }
      $p.Add("The calls differ from the order of operator.md's table at call $($i + 1) of $($want.Count) (expected $(if ($i -lt $want.Count) { $want[$i] } else { 'no more' }), got $(if ($i -lt $got.Count) { $got[$i] } else { 'none' }); $($got.Count) calls)")
    }
    Write-HpProblems "${sy}: $($Arguments -join ' ') starts the host, then makes the table's calls in order: the staging folder removed, box/system and the CA key copied, the install, the CLAUDE.md folder and copy, the build, its copy and install, and the push (steps 2 to 8)" $p

    $ssh = @($calls | Where-Object { $_['kind'] -eq 'ssh' })
    $scp = @($calls | Where-Object { $_['kind'] -eq 'scp' })
    $p = New-HpProblems
    if ($scp.Count -ne 4 -or $ssh.Count -ne 5) { $p.Add("There are $($scp.Count) scp and $($ssh.Count) ssh calls, not 4 and 5") }
    else {
      foreach ($i in 0, 1, 2) {
        $a = @($ssh[$i]['a'])
        if ($ssh[$i]['class'] -ne 'sync' -or $a.Count -ne 4 -or $a[0] -cne '-o' -or $a[1] -cne $caf -or $a[2] -cne $Alias) { $p.Add("ssh call $($i + 1) is not -o ClearAllForwardings=yes $Alias <remote command>") } else { Add-RemoteCommandProblems $p $a "ssh call $($i + 1)" }
      }
      $a = @($ssh[3]['a'])
      if ($ssh[3]['class'] -ne 'sync' -or $a.Count -ne 4 -or $a[2] -cne $Alias) { $p.Add('The ssh that installs the localswim CLI is not through the host''s alias') } else { Add-RemoteCommandProblems $p $a 'the localswim install' }
      $a = @($ssh[4]['a'])
      if ($ssh[4]['class'] -ne 'push' -or -not (Test-Same $a[0..4] @('-o', $caf, '-o', 'BatchMode=yes', $Alias)) -or $a.Count -ne 6) { $p.Add('The push''s ssh is not -o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote command>') }
      $a = @($scp[0]['a'])
      if ($a.Count -ne 6 -or -not (Test-Same $a[0..3] @('-o', $caf, '-q', '-r')) -or $a[5] -cne "${Alias}:$staging") { $p.Add("scp of box/system is not -o ClearAllForwardings=yes -q -r <folder> ${Alias}:$staging") }
      $a = @($scp[1]['a'])
      if ($a.Count -ne 5 -or -not (Test-Same $a[0..2] @('-o', $caf, '-q')) -or -not (Test-HpSamePath $a[3] $ca) -or $a[4] -cne "${Alias}:$staging/agentspace_ca.pub") { $p.Add("scp of the CA's public key is not ... %USERPROFILE%\.agentspace\ca\agentspace_ca.pub ${Alias}:$staging/agentspace_ca.pub") }
      $a = @($scp[2]['a'])
      if ($a.Count -ne 5 -or -not (Test-Same $a[0..2] @('-o', $caf, '-q')) -or -not (Test-HpSamePath $a[3] $claude) -or $a[4] -cne "${Alias}:.claude/CLAUDE.md") { $p.Add("scp of CLAUDE.md is not ... %USERPROFILE%\.claude\CLAUDE.md ${Alias}:.claude/CLAUDE.md") }
      $a = @($scp[3]['a'])
      if ($a.Count -ne 5 -or -not (Test-Same $a[0..2] @('-o', $caf, '-q')) -or $a[4] -cne "${Alias}:/tmp/localswim") { $p.Add("scp of the localswim CLI is not ... <build> ${Alias}:/tmp/localswim") }
    }
    Write-HpProblems "${sy}: every ssh and scp of $($Arguments -join ' ') names the alias $Alias with exactly the arguments of operator.md's table, the push's ssh with BatchMode (steps 1 to 9; Its ssh and scp calls)" $p

    $p = New-HpProblems
    $goCall = @($calls | Where-Object { $_['kind'] -eq 'go' }) | Select-Object -First 1
    $hexes = [System.Collections.Generic.List[string]]::new()
    if ($scp.Count -ge 4 -and $goCall) {
      $out = $goCall['a'][([Array]::IndexOf($goCall['a'], '-o') + 1)]
      if (-not (Test-Same @($goCall['a']) @('build', '-trimpath', '-o', $out, './cmd/localswim'))) { $p.Add('The build is not exactly go build -trimpath -o <out> ./cmd/localswim') }
      if ($goCall['goenv']['GOOS'] -cne 'linux' -or $goCall['goenv']['GOARCH'] -cne 'arm64' -or $goCall['goenv']['CGO_ENABLED'] -cne '0') { $p.Add('The build is not run with GOOS=linux, GOARCH=arm64 and CGO_ENABLED=0') }
      $hexes.Add([string](Get-HpOperatorHex $h $scp[0]['a'][4]))
      $hexes.Add([string](Get-HpOperatorHex $h $out))
      $hexes.Add([string](Get-HpOperatorHex $h $scp[3]['a'][3]))
      if (($hexes | Where-Object { -not $_ }).Count -gt 0) { $p.Add('The system folder and the build are not under %USERPROFILE%\.agentspace\operator\<32 lowercase hex>\') }
      elseif (@($hexes | Select-Object -Unique).Count -ne 1) { $p.Add('The system folder and the build are in different folders: a command has one') }
      if (-not (Test-HpSamePath $scp[3]['a'][3] $out)) { $p.Add('The copy of the localswim CLI is not of the file the build wrote') }
      $sys = @($scp[0]['paths'])[0]
      $map = Get-HpRepoSystemMap
      if ($null -eq $sys -or $sys['type'] -ne 'dir') { $p.Add('The system folder was not a folder at the time of the copy') }
      elseif ((($sys['files'].Keys | Sort-Object) -join ',') -cne (($map.Keys | Sort-Object) -join ',')) { $p.Add('The system folder does not hold exactly the repository''s box\system files') }
      else { foreach ($k in $map.Keys) { if ($sys['files'][$k] -cne $map[$k]) { $p.Add("system/$k is not the repository's file"); break } } }
      $bin = @($scp[3]['paths'])[0]
      if ($null -eq $bin -or $bin['type'] -ne 'file') { $p.Add('The build''s output was not a file at the time of its copy') }
    } else { $p.Add('The system folder, the build and its copy were not all seen') }
    if (@(Get-HpOperatorFolders $h).Count -ne 0) { $p.Add('A folder under operator\ was left after the command ended') }
    Write-HpProblems "${sy}: $($Arguments -join ' ') writes box/system whole and builds the localswim CLI into one operator\<hex>\ folder that its stand-ins see, as go build -trimpath -o <out> ./cmd/localswim with GOOS, GOARCH and CGO_ENABLED, and removes it as it ends (step 7; The operator's folder, one per command)" $p

    $p = New-HpProblems
    $iCopy = [Array]::IndexOf($lines, 'Copying box/system to the instance...')
    $iCa = -1; for ($k = 0; $k -lt $lines.Count; $k++) { if ($lines[$k] -cmatch '^Copying the agentspace SSH CA''s public key \(SHA256:[A-Za-z0-9+/]+\) to the instance\.\.\.$') { $iCa = $k; break } }
    $iMd = [Array]::IndexOf($lines, 'Copying ~/.claude/CLAUDE.md to the instance...')
    $iBuild = [Array]::IndexOf($lines, 'Building the localswim CLI for linux/arm64...')
    $iCli = [Array]::IndexOf($lines, 'Copying the localswim CLI to the instance...')
    $iPush = [Array]::IndexOf($lines, 'Sending the localswim Bot token to the instance...')
    if (-not (0 -le $iCopy -and $iCopy -lt $iCa -and $iCa -lt $iMd -and $iMd -lt $iBuild -and $iBuild -lt $iCli -and $iCli -lt $iPush)) { $p.Add('The progress lines are not Copying box/system, Copying the agentspace SSH CA''s public key (<fp>), Copying ~/.claude/CLAUDE.md, Building, Copying the localswim CLI and Sending the localswim Bot token, in that order') }
    $awsCalls = @($calls | Where-Object { $_['kind'] -eq 'aws' })
    if ($awsCalls.Count -eq 0 -or @($awsCalls | Where-Object { $_['a'] -notcontains $Id }).Count -gt 0) { $p.Add("An AWS call does not name this host's instance") }
    if ($IsBox) {
      if (Test-Path -LiteralPath (Join-Path $h.UserProfile '.agentspace\hosts\swimcoach\id_ed25519')) { $p.Add('A key was made for the box') }
      if (Test-Path -LiteralPath (Join-Path $h.SshDir 'agentspace.d')) { $p.Add('An alias was written for the box') }
    } else {
      foreach ($f in 'id_ed25519', 'id_ed25519-cert.pub') { if (-not (Test-Path -LiteralPath (Join-Path (Get-HpHostFolder $h 'agentspace') $f) -PathType Leaf)) { $p.Add("hosts\agentspace\$f was not made") } }
      if (-not (Test-Path -LiteralPath (Get-HpAliasFile $h $Alias) -PathType Leaf)) { $p.Add('The host''s operator alias was not written') }
    }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpHostsFile $h)) $hostsBefore)) { $p.Add('The registry was written: sync writes no record') }
    Write-HpProblems "${sy}: $($Arguments -join ' ') prints its progress lines in order, names only this host's instance to AWS, $(if ($IsBox) { 'writes no key, certificate or alias for the box' } else { 'makes the operator''s key, certificate and alias for the host' }), and writes no record" $p
  }
}
Test-SyncCalls 'sync -Host agentspace' @('sync', '-Host', 'agentspace') $alias $script:HpHostId $false
Test-SyncCalls 'sync, for the box' @('sync') 'agentspace' $script:HpBoxId $true
Test-SyncCalls 'sync -Host swimcoach, for the box' @('sync', '-Host', 'swimcoach') 'agentspace' $script:HpBoxId $true

Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the ProxyCommand finding a host by its project and its platform (docs/operator.md,
# "From the ProxyCommand", "The ProxyCommand", "A lease that is not ready (#0229)", "The stamp", "A host's
# platform and its names"; docs/state.md, "A host's operator alias", "Agentspace's host keys", "A host's push
# stamp", "Which host a connection reaches"; docs/conformance.md, "v0.12's planned cases for the shared and
# Linux parts", cases W16 to W18; D49, D50, D91; E0.12-T04-S02, card #0262, planned by #0248). Every
# expectation is the spec's text: which record an alias reaches, the refusal that is not the box, and the
# stamp and the instance ID written into the platform's own host.
#
# What it does: as test-unmet-host-proxy-lookup.ps1 does (conformance/operator/host-proxy-common.ps1): it
# runs the ProxyCommand against a stand-in `aws` that answers a running instance, starts the tunnel by
# printing a marker, and answers `describe-stacks` from a file, with ssh and scp tripwires (or a stand-in ssh
# that answers the token push as the box does). It seeds the host registry and the lease state file in a
# throwaway home. No case starts a Windows host.
#
#   W16  A lease's host is found by the lease record's project and platform, never from its alias: a `ready`
#        linux/amd64 lease reaches the project's amd64 host whatever its arm64 record is; with no `ready`
#        amd64 host the connection is refused, never sent to the box; a `ready` linux/arm64 lease with no
#        `ready` arm64 record still reaches the box; a lease that is not `ready` is refused.
#   W17  A host alias carries the platform: agentspace-host.<project>.linux-amd64 is the amd64 host, whatever
#        its state; two records whose names look alike resolve to their own; a platform part that is not
#        linux-amd64 or windows-amd64 fails as a failed lookup.
#   W18  What a connection writes, it writes to its platform's host: a missing instance ID, read from that
#        platform's stack, and the push stamp at hosts\<project>.linux-amd64\.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential, and the check confirms first that `aws sts get-caller-identity` fails under
# that environment for every AWS CLI the operator can run (SKIPPED if not). Every ID is made up. A failure
# reason never quotes the program's output.
#
# The programs under test are the ones AGENTSPACE_SSH_PROXY_CMD and AGENTSPACE_OPERATOR_CMD name; unset,
# each takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-proxy.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$sec = 'operator.md#from-the-proxycommand'
$stampSec = 'state.md#a-hosts-push-stamp'
$zone = 'America/New_York'

# The problems of a refusal that is not "Could not reach the host": exit 1, nothing on stdout, no call the
# stand-in aws records, no tunnel, no ssh or scp, and exactly one line on stderr, which starts `agentspace: `,
# holds each of $Words and names no host (no instance ID, no host alias).
function Get-PxRefusedProblems($Run, $S, [string[]]$Words) {
  $p = New-HpProblems
  if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
  if ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
  if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
  $a = Get-HpAsked $S
  if ($a.Raw.Count -gt 0) { $p.Add('The stand-in aws was called') }
  if ($a.Tunnels.Count -gt 0) { $p.Add('A tunnel was started') }
  if ((Get-HpSshCallCount $S) -gt 0 -or (Get-StandInCallCount -Log (Join-Path $S.Dir 'scp-tripwire-calls.log')) -gt 0) { $p.Add('ssh or scp was started') }
  $lines = @($Run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
  if ($lines.Count -ne 1) { $p.Add("Stderr has $($lines.Count) lines, not the refusal's one"); return , $p }
  $line = $lines[0].Trim()
  if (-not $line.StartsWith('agentspace: ', [StringComparison]::Ordinal)) { $p.Add('The line does not start with "agentspace: "') }
  foreach ($w in $Words) { if (-not $line.Contains($w)) { $p.Add("The line does not hold '$(ConvertTo-ConformanceMasked $w)'") } }
  if ($line -match 'agentspace-host\.|\bi-[0-9a-f]{8,}|linux-amd64|agentbox|agentspace-linux') { $p.Add('The line names a host') }
  , $p
}

$idArmLease = '1111111a-1111-4111-8111-11111111111a'
$idAmdLease = '2222222b-2222-4222-8222-22222222222b'
$idRelAmd = '3333333c-3333-4333-8333-33333333333c'
$idPendAmd = '4444444d-4444-4444-8444-44444444444d'
$idUnsatAmd = '5555555e-5555-4555-8555-55555555555e'
$idExpAmd = '6666666f-6666-4666-8666-66666666666f'
$idNoArm = '7777777a-7777-4777-8777-77777777777a'
$idCrArm = '8888888b-8888-4888-8888-88888888888b'
$future = [DateTime]::new(2099, 1, 1, 0, 0, 0, [DateTimeKind]::Utc)

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the ProxyCommand finds a lease's host by the lease record's project and platform"
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $operator = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program -or -not $operator) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither a conformance variable nor its default names the ProxyCommand program or the operator'
  } else {
    $S = New-HpStandIns $h
    $leasesPath = Get-HpLeasesPath $h
    $armRec = New-PfRecord -Project 'foo' -InstanceId $HpIdOther
    $ampRec = New-PfRecord -Project 'foo' -Platform 'linux/amd64' -InstanceId $HpIdOther2
    $lArm = New-PfLease foo ready -Expires $future -Id $idArmLease
    $lAmd = New-PfLease foo ready -Platform 'linux/amd64' -Expires $future -Id $idAmdLease
    $aliasArm = Get-PfLeaseAlias $lArm
    $aliasAmd = Get-PfLeaseAlias $lAmd
    # A lease alias is agentspace-<project>-<8 hex digits> on every platform (pin P7), so the two differ in their digits alone.
    $p = New-HpProblems
    if ($aliasArm -cne 'agentspace-foo-1111111a' -or $aliasAmd -cne 'agentspace-foo-2222222b') { $p.Add('A lease alias is not agentspace-<project>-<first 8 hex digits of its ID>') }
    Write-HpProblems "${sec}: W16: a lease's alias is agentspace-<project>-<8 hex digits> on every platform, the platform in no part of it (pin P7)" $p

    # ============ W16 ============
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $armRec, $ampRec)
    Set-HpLeases $h @($lArm, $lAmd)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd)
    Write-HpProblems "${sec}: W16: a ready linux/amd64 lease's alias reaches the amd64 record's instance, and the arm64 record's instance is not asked about" (Get-HpReachedProblems $r $S $HpIdOther2)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasArm)
    Write-HpProblems "${sec}: W16: a ready linux/arm64 lease's alias of the same project reaches the arm64 record's instance, and never the amd64 host" (Get-HpReachedProblems $r $S $HpIdOther)
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-PfRecord -Project 'foo' -State 'failed' -InstanceId $HpIdFailed), $ampRec)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd)
    Write-HpProblems "${sec}: W16: the amd64 lease's host is the amd64 record, whatever the project's arm64 record, instance ID or state is (a failed arm64 record)" (Get-HpReachedProblems $r $S $HpIdOther2)
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $ampRec, $armRec)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd)
    Write-HpProblems "${sec}: W16: the amd64 lease's host is found by platform, not by the file's order: with the amd64 record before the arm64 one it is the same" (Get-HpReachedProblems $r $S $HpIdOther2)

    # No ready record on the lease's platform: refused, never the box.
    $line = { param([string]$Alias) "agentspace: The lease for $Alias has no ready host on its platform; no connection was opened." }
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $armRec)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd)
    $p = Get-PxRefusedProblems $r $S @('no ready host on its platform', 'no connection was opened', $aliasAmd)
    if ($r.Stderr.Trim() -cne (& $line $aliasAmd)) { $p.Add('The line is not the spec''s: agentspace: The lease for <alias> has no ready host on its platform; no connection was opened.') }
    Write-HpProblems "${sec}: W16: a ready linux/amd64 lease of a project with only its arm64 record is refused, never sent to the box: exit 1, nothing on stdout, one line on stderr naming no host, no tunnel, and no call the stand-in aws records" $p
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $armRec, (New-PfRecord -Project 'foo' -Platform 'linux/amd64' -State 'creating' -InstanceId $HpIdThird))
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd)
    Write-HpProblems "${sec}: W16: a ready linux/amd64 lease whose amd64 record is not ready (creating) is refused the same way, never sent to the box, and its instance is not asked about" (Get-PxRefusedProblems $r $S @('no ready host on its platform', 'no connection was opened', $aliasAmd))
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox))
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasAmd) @{ AGENTSPACE_DIAGNOSTIC = '1' }
    $p = Get-PxRefusedProblems $r $S @('no connection was opened')
    $last = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' }) | Select-Object -Last 1
    if (@($p | Where-Object { $_ -like 'Stderr has*' }).Count -eq 1) { $p.RemoveAll({ param($x) $x -like 'Stderr has*' }) | Out-Null }
    if ($last -cne (& $line $aliasAmd)) { $p.Add('With AGENTSPACE_DIAGNOSTIC set, the refusal is not the last line of stderr') }
    Write-HpProblems "${sec}: W16: a ready linux/amd64 lease of a project with no record at all is refused, and AGENTSPACE_DIAGNOSTIC adds no line to the refusal: it stays the last" $p

    # A ready linux/arm64 lease with no ready arm64 record still reaches the box.
    $lNoArm = New-PfLease foo ready -Expires $future -Id $idNoArm
    $lCrArm = New-PfLease foo ready -Expires $future -Id $idCrArm
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $ampRec)
    Set-HpLeases $h @($lNoArm)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @((Get-PfLeaseAlias $lNoArm))
    Write-HpProblems "${sec}: W16: a ready linux/arm64 lease whose project has no arm64 record, though it has an amd64 one, reaches the box as before v0.12" (Get-HpReachedProblems $r $S $HpIdBox)
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-PfRecord -Project 'foo' -State 'creating' -InstanceId $HpIdThird), $ampRec)
    Set-HpLeases $h @($lCrArm)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @((Get-PfLeaseAlias $lCrArm))
    Write-HpProblems "${sec}: W16: a ready linux/arm64 lease whose arm64 record is not ready (creating) reaches the box as before v0.12" (Get-HpReachedProblems $r $S $HpIdBox)

    # A lease that is not ready is refused as v0.11's case refuses it, whatever its platform.
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $armRec, $ampRec)
    $notReady = @(
      @{ State = 'released'; Rec = (New-PfLease foo released -Platform 'linux/amd64' -Released (Get-PfAgo 5) -Expires (Get-PfAgo 30) -Id $idRelAmd) },
      @{ State = 'pending'; Rec = (New-PfLease foo pending -Platform 'linux/amd64' -Id $idPendAmd) },
      @{ State = 'unsatisfiable'; Rec = (New-PfLease foo unsatisfiable -Platform 'linux/amd64' -Id $idUnsatAmd) },
      @{ State = 'expired'; Rec = (New-PfLease foo ready -Platform 'linux/amd64' -Expires (Get-PfAgo 10) -Id $idExpAmd) })
    foreach ($c in $notReady) {
      $c1 = $c
      Set-HpLeases $h @($lAmd, $c1.Rec)
      $alias = 'agentspace-foo-' + ((ConvertFrom-Json -InputObject $c1.Rec).id).Substring(0, 8)
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($alias)
      $p = Get-PxRefusedProblems $r $S @($c1.State, 'no connection was opened', $alias)
      Write-HpProblems "${sec}: W16: a $($c1.State) linux/amd64 lease is refused as v0.11 refuses a lease that is not ready, whatever its platform: exit 1, one line naming its state, no tunnel, no call" $p
    }

    # ============ W17 ============
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), $armRec, (New-PfRecord -Project 'foo' -Platform 'linux/amd64' -State 'creating' -InstanceId $HpIdThird))
    Set-HpLeases $h @($lArm)
    foreach ($c in @(
        @{ N = 'agentspace-host.foo is the arm64 host'; A = 'agentspace-host.foo'; Want = $HpIdOther },
        @{ N = 'agentspace-host.foo.linux-amd64 is the amd64 host, whatever its state, so the operator''s own connections reach a host still creating'; A = 'agentspace-host.foo.linux-amd64'; Want = $HpIdThird })) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($c.A)
      Write-HpProblems "${sec}: W17: $($c.N)" (Get-HpReachedProblems $r $S $c.Want)
    }
    foreach ($c in @(
        @{ N = 'a platform part that is linux-arm64, which never appears in a name'; A = 'agentspace-host.foo.linux-arm64' },
        @{ N = 'a platform part that is windows-arm64, which is no platform'; A = 'agentspace-host.foo.windows-arm64' },
        @{ N = 'a platform part in capitals, LINUX-AMD64'; A = 'agentspace-host.foo.LINUX-AMD64' },
        @{ N = 'a platform part windows-amd64 for a project with no record on it'; A = 'agentspace-host.foo.windows-amd64' },
        @{ N = 'a project with no record on the platform, agentspace-host.nobody.linux-amd64'; A = 'agentspace-host.nobody.linux-amd64' },
        @{ N = 'two dots, so that the part before the last is no project, agentspace-host.foo.linux-amd64.linux-amd64'; A = 'agentspace-host.foo.linux-amd64.linux-amd64' })) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($c.A)
      Write-HpProblems "${sec}: W17: $($c.N) fails as a failed lookup does, with the caller's Could not reach the host line and no tunnel" (Get-HpRefusedProblems $r $S)
    }
    # Two records whose names look alike resolve to their own, by the last `.` of the alias.
    $stackA = 'agentspace-host-foo-linux-amd64'
    $stackB = 'agentspace-linux-amd64-host-foo'
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-PfRecord -Project 'foo-linux-amd64'), (New-PfRecord -Project 'foo' -Platform 'linux/amd64'))
    Clear-HpStacks $h $S
    Set-HpStack $h $S $stackA $HpIdSpare
    Set-HpStack $h $S $stackB $HpIdOther2
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.foo-linux-amd64')
    $p = Get-HpReachedProblems $r $S $HpIdSpare -StacksAllowed
    if ((Get-HpAsked $S).Stacks -join ',' -cne $stackA) { $p.Add('The stack asked about is not agentspace-host-foo-linux-amd64, the arm64 record of the project foo-linux-amd64') }
    Write-HpProblems "${sec}: W17: with the arm64 record of a project foo-linux-amd64 and the amd64 record of a project foo, agentspace-host.foo-linux-amd64 reaches the first, by the stack agentspace-host-foo-linux-amd64" $p
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.foo.linux-amd64')
    $p = Get-HpReachedProblems $r $S $HpIdOther2 -StacksAllowed
    if ((Get-HpAsked $S).Stacks -join ',' -cne $stackB) { $p.Add('The stack asked about is not agentspace-linux-amd64-host-foo, the amd64 record of the project foo') }
    Write-HpProblems "${sec}: W17: and agentspace-host.foo.linux-amd64 reaches the second, by the stack agentspace-linux-amd64-host-foo: the two stack names are two (D91)" $p

    # ============ W18 ============
    $instSec = 'operator.md#the-host-registry-as-the-operator-uses-it'
    Clear-HpStacks $h $S
    Set-HpStack $h $S $stackB $HpIdOther2
    Set-HpStack $h $S 'agentspace-host-foo' $HpIdOther
    $armNoId = New-PfRecord -Project 'foo'
    $ampNoId = New-PfRecord -Project 'foo' -Platform 'linux/amd64'
    $boxRec = New-HpHostRecord 'swimcoach' 'ready' $HpIdBox
    Set-HpRegistry $h @($boxRec, $armNoId, $ampNoId)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.foo.linux-amd64')
    $p = Get-HpReachedProblems $r $S $HpIdOther2 -StacksAllowed
    if ((Get-HpAsked $S).Stacks -join ',' -cne $stackB) { $p.Add('The stack read for the missing instance ID is not the amd64 host''s, agentspace-linux-amd64-host-foo') }
    $want = New-HpRegistryText @($boxRec, $armNoId, (New-PfRecord -Project 'foo' -Platform 'linux/amd64' -InstanceId $HpIdOther2))
    if ((Get-HpText (Get-HpRegistryPath $h)) -cne $want) { $p.Add('hosts.json is not the three records as they were, with the instance ID written into the amd64 record alone') }
    Write-HpProblems "${instSec}: W18: a missing instance ID is read from the connection's platform's stack and written into that record alone: the arm64 record is never touched" $p
    Set-HpRegistry $h @($boxRec, $armNoId, $ampNoId)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.foo')
    $p = Get-HpReachedProblems $r $S $HpIdOther -StacksAllowed
    $want = New-HpRegistryText @($boxRec, (New-PfRecord -Project 'foo' -InstanceId $HpIdOther), $ampNoId)
    if ((Get-HpText (Get-HpRegistryPath $h)) -cne $want) { $p.Add('hosts.json is not the three records as they were, with the instance ID written into the arm64 record alone') }
    Write-HpProblems "${instSec}: W18: and an arm64 connection writes the arm64 record alone: the amd64 record keeps its bytes" $p

    # The push is due or not by the platform's host's own stamp, and writes it.
    $S.Ssh = New-HpPushSsh $h $zone
    $fx = New-HpPushFixture $h
    $agentspaceDir = Join-Path $h.UserProfile '.agentspace'
    $boxStamp = Join-Path $agentspaceDir 'localswim-pushed.json'
    $armStamp = Join-Path $agentspaceDir 'hosts\foo\localswim-pushed.json'
    $ampStamp = Join-Path $agentspaceDir 'hosts\foo.linux-amd64\localswim-pushed.json'
    $push = @{ AGENTSPACE_TIMEZONE = $zone }
    Set-HpRegistry $h @($boxRec, $armRec, $ampRec)
    Set-HpLeases $h @($lArm, $lAmd)
    function Invoke-Push([string[]]$Arguments) { foreach ($f in Get-ChildItem -LiteralPath $S.Dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'aws-*.log' }) { Remove-HpPath $h $f.FullName }; Invoke-HpProxy $h $S $Arguments $push -Push }
    function Test-PushTo($Run, [string]$ExpectAlias, [int]$CallsBefore) {
      $p = New-HpProblems
      if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
      if ((Get-HpPushCount $S) -ne $CallsBefore + 1) { $p.Add("The push ssh was started $((Get-HpPushCount $S) - $CallsBefore) times, not once"); return , $p }
      $a = Get-HpPushArgs $S ($CallsBefore + 1)
      if ($null -eq $a -or $a.Count -ne 6) { $p.Add('The push ssh did not get six arguments'); return , $p }
      $wantArgs = @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $ExpectAlias)
      for ($i = 0; $i -lt 5; $i++) { if ($a[$i] -cne $wantArgs[$i]) { $p.Add("Argument $($i + 1) of the push ssh is not as the spec gives it") } }
      , $p
    }
    function Test-StampOf([string]$Path, [string]$Id, [string]$Label) {
      $p = New-HpProblems
      $v = Get-HpStampValue $Path
      if ($null -eq $v) { $p.Add("No stamp file for $Label") }
      elseif ($v -cnotmatch '\A[0-9A-F]{64}\z') { $p.Add("The stamp for $Label is not {""version"":0,""stamp"":""<64 uppercase hex digits>""}") }
      elseif ($v -cne (Get-HpExpectedStamp $fx $Id $zone)) { $p.Add("The stamp for $Label is not the SHA-256 of <$Label's instance ID> LF <zone> LF <content>") }
      , $p
    }
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.foo.linux-amd64')
    $p = Test-PushTo $r 'agentspace-host.foo.linux-amd64' $n
    foreach ($x in (Test-StampOf $ampStamp $HpIdOther2 'the amd64 host')) { $p.Add($x) }
    if (Test-Path -LiteralPath $armStamp) { $p.Add('The arm64 host''s stamp was written by a push to the amd64 host') }
    if (Test-Path -LiteralPath $boxStamp) { $p.Add('The box''s stamp was written by a push to the amd64 host') }
    Write-HpProblems "${stampSec}: W18: the amd64 host is pushed over its own alias and its stamp is hosts\<project>.linux-amd64\localswim-pushed.json over its own instance ID; no other host's stamp and not the box's is written" $p
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.foo.linux-amd64')
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A push that is not due started an ssh') }
    foreach ($x in (Test-StampOf $ampStamp $HpIdOther2 'the amd64 host')) { $p.Add($x) }
    foreach ($x in (Get-HpReachedProblems $r $S $HpIdOther2 -StacksAllowed)) { $p.Add($x) }
    Write-HpProblems "${sec}: W18: the amd64 host's current stamp sends nothing the next connection" $p
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.foo')
    $p = Test-PushTo $r 'agentspace-host.foo' $n
    foreach ($x in (Test-StampOf $armStamp $HpIdOther 'the arm64 host')) { $p.Add($x) }
    Write-HpProblems "${sec}: W18: the project's arm64 host is pushed though the amd64 host's stamp is current: each host's own stamp alone governs its push" $p
    $n = Get-HpPushCount $S
    $r = Invoke-Push @($aliasAmd)
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A lease alias of an amd64 host whose stamp is current started a push') }
    foreach ($x in (Get-HpReachedProblems $r $S $HpIdOther2 -StacksAllowed)) { $p.Add($x) }
    Write-HpProblems "${sec}: W18: a ready amd64 lease's alias reaches the amd64 host, so its stamp decides: one answer for the connection and the push" $p
    $n = Get-HpPushCount $S
    $r = Invoke-Push @()
    $p = Test-PushTo $r 'agentspace' $n
    foreach ($x in (Test-StampOf $boxStamp $HpIdBox 'the box')) { $p.Add($x) }
    if ((Get-HpStampValue $ampStamp) -cne (Get-HpExpectedStamp $fx $HpIdOther2 $zone)) { $p.Add('The amd64 host''s stamp changed when the box was pushed') }
    Write-HpProblems "${stampSec}: W18: the box's stamp stays where it was, at %USERPROFILE%\.agentspace\localswim-pushed.json, and the amd64 host's stamp is untouched" $p
  }
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

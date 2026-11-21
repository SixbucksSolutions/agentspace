#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the push stamp per host (docs/operator.md, "The token push", "The stamp";
# docs/state.md, "A host's push stamp", "The push stamp"; "Which checks show it", the row for the push
# stamp per host, the box's staying where it was), run through the test seam. Story E0.07-T04-S02 (#0156).
#
# The push is made by the ProxyCommand (with AGENTSPACE_NESTED unset) and by a person's `up`, with a
# stand-in `ssh` that acts as the box does, a made-up token, a fake receipt and a DPAPI credential
# under the throwaway home, and AGENTSPACE_TIMEZONE set. It checks, from outside:
#   - a host reached by its host alias is pushed over that alias and its stamp is
#     %USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json, the SHA-256 of that host's own
#     instance ID, the zone and the content as uppercase hex; the box's stamp is not written;
#   - the box (no argument) is pushed over `agentspace` and its stamp stays at
#     %USERPROFILE%\.agentspace\localswim-pushed.json, covering the box's ID, and no stamp is written
#     in hosts\swimcoach\ or in any other host's folder;
#   - each host is pushed once per change: a stamp that is current sends nothing, the other host's
#     stamp never makes a push due or stops one, an instance ID that changes makes the push due again,
#     and a lease alias of a ready host reaches that host and so is pushed, or not, by that host's stamp;
#   - a person's `up -Host <project> -Alias <alias>` pushes over that alias, with the host's stamp, and
#     `up -Host swimcoach -Alias <alias>` the same for the box with the box's stamp.
#
# SAFETY. The token is a made-up value. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA
# into the throwaway home and removes every AWS credential, so a mistaken run cannot find Terry's
# receipt or token, and the check confirms first that `aws sts get-caller-identity` fails under that
# environment for every AWS CLI the operator can run (SKIPPED if not). The throwaway CA signs the
# operator's certificate for `up -Host`; none of it reaches a host. A failure reason never quotes the
# program's output, and no reason holds the token.
#
# The programs under test are the ones AGENTSPACE_SSH_PROXY_CMD and AGENTSPACE_OPERATOR_CMD name; unset,
# each takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program
# under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-proxy-stamp.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'operator.md#the-token-push'
$stampSec = 'state.md#a-hosts-push-stamp'
$leaseOther = '1111111a-1111-4111-8111-11111111111a'
$aliasOther = 'agentspace-other-1a2b3c4d'
$zone = 'America/New_York'

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the push stamp is kept per host"
  $proxy = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $operator = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $proxy -or -not $operator) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither a conformance variable nor its default names the ProxyCommand program or the operator'
  } else {
    $S = New-HpStandIns $h
    $S.Ssh = New-HpPushSsh $h $zone
    $fx = New-HpPushFixture $h
    $agentspaceDir = Join-Path $h.UserProfile '.agentspace'
    $boxStamp = Join-Path $agentspaceDir 'localswim-pushed.json'
    $otherStamp = Join-Path $agentspaceDir 'hosts\other\localswim-pushed.json'
    $swimStamp = Join-Path $agentspaceDir 'hosts\swimcoach\localswim-pushed.json'
    $thirdStamp = Join-Path $agentspaceDir 'hosts\third\localswim-pushed.json'
    $push = @{ AGENTSPACE_TIMEZONE = $zone }
    Set-HpRegistry $h @(
      (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox),
      (New-HpHostRecord 'other' 'ready' $HpIdOther),
      (New-HpHostRecord 'third' 'creating' $HpIdThird))
    Set-HpLeases $h @(
      (New-HpLeaseRecord $leaseOther 'other' $aliasOther),
      (New-HpLeaseRecord '2222222b-2222-4222-8222-22222222222b' 'third' 'agentspace-third-1a2b3c4e'))

    function Test-Push($Run, [string]$ExpectAlias, [int]$CallsBefore) {
      $p = New-HpProblems
      if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
      if ((Get-HpPushCount $S) -ne $CallsBefore + 1) { $p.Add("The push ssh was started $((Get-HpPushCount $S) - $CallsBefore) times, not once") ; return , $p }
      $a = Get-HpPushArgs $S ($CallsBefore + 1)
      if ($null -eq $a -or $a.Count -ne 6) { $p.Add('The push ssh did not get six arguments'); return , $p }
      $want = @('-o', 'ClearAllForwardings=yes', '-o', 'BatchMode=yes', $ExpectAlias)
      for ($i = 0; $i -lt 5; $i++) { if ($a[$i] -cne $want[$i]) { $p.Add("Argument $($i + 1) of the push ssh is not as the spec gives it (the alias is the host's own, or the one given)") } }
      , $p
    }
    function Test-Stamp([string]$Path, [string]$Id, [string]$Label) {
      $p = New-HpProblems
      $v = Get-HpStampValue $Path
      if ($null -eq $v) { $p.Add("No stamp file for $Label") }
      elseif ($v -cnotmatch '\A[0-9A-F]{64}\z') { $p.Add("The stamp for $Label is not {""version"":0,""stamp"":""<64 uppercase hex digits>""}") }
      elseif ($v -cne (Get-HpExpectedStamp $fx $Id $zone)) { $p.Add("The stamp for $Label is not the SHA-256 of <$Label's instance ID> LF <zone> LF <content>") }
      , $p
    }
    function Invoke-Push([string[]]$Arguments) { Clear-HpLogsExceptSsh; Invoke-HpProxy $h $S $Arguments $push -Push }
    function Clear-HpLogsExceptSsh {
      foreach ($f in Get-ChildItem -LiteralPath $S.Dir -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -like 'aws-*.log' }) { Remove-HpPath $h $f.FullName }
    }

    # ---- The first push to a host: its alias, its stamp ----
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.other')
    $p = Test-Push $r 'agentspace-host.other' $n
    foreach ($x in (Test-Stamp $otherStamp $HpIdOther 'the host')) { $p.Add($x) }
    if (Test-Path -LiteralPath $boxStamp) { $p.Add('The box''s stamp was written by a push to another host') }
    if (Test-Path -LiteralPath $swimStamp) { $p.Add('A stamp was written in hosts\swimcoach') }
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not the tunnel's 0") }
    Write-HpProblems "${stampSec}: A host reached by its host alias is pushed over that alias, and its stamp is hosts\<project>\localswim-pushed.json over the host's own ID; the box's stamp is not written" $p

    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.other')
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A push that is not due started an ssh') }
    Write-HpProblems "${sec}: A host's current stamp sends nothing the next connection (The stamp)" $p

    # ---- The box: its alias, its stamp where it was ----
    $n = Get-HpPushCount $S
    $r = Invoke-Push @()
    $p = Test-Push $r 'agentspace' $n
    foreach ($x in (Test-Stamp $boxStamp $HpIdBox 'the box')) { $p.Add($x) }
    if (Test-Path -LiteralPath $swimStamp) { $p.Add('A stamp was written in hosts\swimcoach, where the box''s stamp does not live') }
    $v = Get-HpStampValue $otherStamp
    if ($v -cne (Get-HpExpectedStamp $fx $HpIdOther $zone)) { $p.Add('The other host''s stamp changed when the box was pushed') }
    Write-HpProblems "${stampSec}: The box is pushed over agentspace and its stamp stays at %USERPROFILE%\.agentspace\localswim-pushed.json over the box's ID, the other host's stamp untouched" $p

    $n = Get-HpPushCount $S
    $r = Invoke-Push @()
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A push that is not due started an ssh') }
    Write-HpProblems "${sec}: The box's current stamp sends nothing the next connection, whatever the other host's stamp is" $p

    # Each host's stamp alone governs its push: a host with no stamp is pushed while the others' stand.
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.third')
    $p = Test-Push $r 'agentspace-host.third' $n
    foreach ($x in (Test-Stamp $thirdStamp $HpIdThird 'the host')) { $p.Add($x) }
    Write-HpProblems "${sec}: A host with no stamp is pushed once though the box's and another host's stamps are current (each host is pushed once per change)" $p

    # ---- A changed instance ID makes that host's push due again ----
    Set-HpRegistry $h @(
      (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox),
      (New-HpHostRecord 'other' 'ready' $HpIdOther2),
      (New-HpHostRecord 'third' 'creating' $HpIdThird))
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-host.other')
    $p = Test-Push $r 'agentspace-host.other' $n
    foreach ($x in (Test-Stamp $otherStamp $HpIdOther2 'the host')) { $p.Add($x) }
    Write-HpProblems "${sec}: A host whose instance ID changed is pushed again, and its stamp covers the new ID (The stamp)" $p

    # ---- A lease alias reaches the host, so the host's stamp decides ----
    $n = Get-HpPushCount $S
    $r = Invoke-Push @($aliasOther)
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A lease alias of a host whose stamp is current started a push') }
    Write-HpProblems "${sec}: A lease alias of a ready host reaches that host, so the host's current stamp sends nothing (one answer for the connection and the push)" $p

    Remove-HpPath $h $otherStamp
    $n = Get-HpPushCount $S
    $r = Invoke-Push @($aliasOther)
    $p = Test-Push $r $aliasOther $n
    foreach ($x in (Test-Stamp $otherStamp $HpIdOther2 'the host')) { $p.Add($x) }
    $v = Get-HpStampValue $boxStamp
    if ($v -cne (Get-HpExpectedStamp $fx $HpIdBox $zone)) { $p.Add('The box''s stamp changed when a lease of another host connected') }
    Write-HpProblems "${stampSec}: A lease alias of a ready host with no stamp is pushed over the lease alias, and the stamp is that host's, the box's untouched" $p

    # A lease alias of a host that is not ready falls to the box, and the box's stamp decides.
    $n = Get-HpPushCount $S
    $r = Invoke-Push @('agentspace-third-1a2b3c4e')
    $p = New-HpProblems
    if ((Get-HpPushCount $S) -ne $n) { $p.Add('A lease alias that falls to the box, whose stamp is current, started a push') }
    if (Test-Path -LiteralPath (Join-Path $agentspaceDir 'hosts\third\id_ed25519-cert.pub')) { $p.Add('The ProxyCommand signed a certificate') }
    Write-HpProblems "${sec}: A lease alias of a host that is not ready reaches the box, so the box's stamp decides" $p

    # ---- A person's up, with -Host and -Alias ----
    $name1 = "${sec}: up -Host <project> -Alias <alias> pushes over the alias with the host's stamp"
    if (-not $h.CaReady) {
      Write-ConformanceResult -Name $name1 -Status SKIPPED -Reason $h.CaReason
    } else {
      Remove-HpPath $h $otherStamp
      $n = Get-HpPushCount $S
      Clear-HpLogsExceptSsh
      $r = Invoke-HpOperator $h $S @('up', '-Host', 'other', '-Alias', 'conformance-push-alias') $push
      $p = Test-Push $r 'conformance-push-alias' $n
      foreach ($x in (Test-Stamp $otherStamp $HpIdOther2 'the host')) { $p.Add($x) }
      if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
      if ($r.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
      $v = Get-HpStampValue $boxStamp
      if ($v -cne (Get-HpExpectedStamp $fx $HpIdBox $zone)) { $p.Add('The box''s stamp changed') }
      Write-HpProblems $name1 $p

      Remove-HpPath $h $boxStamp
      $n = Get-HpPushCount $S
      Clear-HpLogsExceptSsh
      $r = Invoke-HpOperator $h $S @('up', '-Host', 'swimcoach', '-Alias', 'conformance-push-alias') $push
      $p = Test-Push $r 'conformance-push-alias' $n
      foreach ($x in (Test-Stamp $boxStamp $HpIdBox 'the box')) { $p.Add($x) }
      if (Test-Path -LiteralPath $swimStamp) { $p.Add('A stamp was written in hosts\swimcoach') }
      $v = Get-HpStampValue $otherStamp
      if ($v -cne (Get-HpExpectedStamp $fx $HpIdOther2 $zone)) { $p.Add('The other host''s stamp changed') }
      Write-HpProblems "${stampSec}: up -Host swimcoach -Alias <alias> pushes over the alias with the box's stamp, where it was, and writes none in hosts\swimcoach" $p
    }

    # ---- The token is in the stamp nowhere ----
    $p = New-HpProblems
    foreach ($f in $boxStamp, $otherStamp, $thirdStamp) {
      $t = Get-HpText $f
      if ($t -and $t.Contains($fx.Token)) { $p.Add('A stamp holds the token') }
    }
    Write-HpProblems "${sec}: No stamp holds the token (Secrecy)" $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-proxy-stamp.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

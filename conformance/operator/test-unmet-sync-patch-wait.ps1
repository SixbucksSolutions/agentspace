#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `sync`'s patch wait (docs/operator.md, "The patch wait", "sync", "What up waits for",
# "Calls by command", "AWS failures", "The test seam"), run through the test seam: E0.08-T04-S01 (#0182),
# from D61's change (#0163) and pins P13, P21 and P24. T03's cases (test-unmet-host-bootstrap-sync.ps1,
# test-unmet-host-bootstrap-sync-all.ps1 and test-unmet-host-provision.ps1) show that the wait's three
# calls come after the install when the host is already patched; this file is the wait's own cases.
#
# EXPECTED TO FAIL AGAINST v0.07's OPERATOR, which has no patch wait in `sync`: every result reports a FAIL
# there but two, which show behavior v0.07 already has: `up` ending its wait at a `failed` answer (same-question's
# second result) and `sync` with a failing install making no wait (install-fails). A v0.08 build MUST pass
# every result line.
#
# What it does: each case makes its own throwaway home (the harness's, with a throwaway CA) holding a host
# registry with the box and one other host, both with an instance ID, a CLAUDE.md and a fake localswim
# receipt, and the stand-ins of conformance/operator/host-provision-common.ps1: an `aws` whose answers
# change from call to call (one list per `<service> <operation>`, each call taking the next answer and
# the last repeating), an `ssh` and an `scp` that record their arguments and exit 0. The host is running
# and was launched long ago, so the start and wait before the install asks nothing, and the calls that
# come after the install's row are the patch wait's alone. `sync` runs no `go`: the check's PATH holds
# Windows' own folder alone, so step 7 prints its skip message, and no case reaches the real `go` or
# anything under C:\Projects\localswim. `ssh-keygen` has no seam (operator.md, Q30): the operator runs
# the real `ssh-keygen.exe` for a host that is not the box, against the throwaway CA in the throwaway home.
#
# Cases (-Case takes a wildcard on these names; the default runs every case but the two on-request ones):
#   patched-at-once, same-question, pending, quiet, not-ready-answers, polls, reboot, sign-in,
#   describe-fails, failed-first, failed-after-pending, failed-all, recent-launch, install-fails,
#   old-launch, failed-later-row, provision-wait, failed-provision, deploy-wait, all, sign-in-all,
#   and, on request only, deadline and deadline-provision.
#
# THE TWO DEADLINE CASES WAIT THE REAL 30 MINUTES EACH, and so they run only when asked for
# (docs/conformance.md, "Cases that run only when asked for", as `up`'s 15-minute deadline case does).
# No seam variable shortens the 30-minute bound (P13), and a check MUST NOT trust a time to be shorter.
# So by default the bound's message and its time are not shown, and this file says so: with `-Case
# deadline`, exactly (in any letter case), the case runs under a limit of its own on `sync` of 34
# minutes, so that it can never hang, and with `-Case deadline-provision`, exactly, the second case
# runs a provisioning run under a limit of 35 minutes. With any other -Case that would select either,
# the default `*` and a wildcard such as `d*` included, it neither runs nor reports it: it writes no
# result line for it, so a default run's results and exit code are those of the other cases alone,
# and prints, for each case it would have selected, the one line that is not a result, `Not run: the
# 30-minute deadline case; run it with -Case deadline.` and `Not run: the 30-minute provisioning
# deadline case; run it with -Case deadline-provision.` The deadline is tested to fall from 29 to 32
# minutes after the run's start: the spec's 30 minutes run from the start of the wait, which begins
# seconds after the run's start, and the stand-ins' own time over several hundred checks adds to it.
# Each case uses a lean `aws` stand-in of its own, since the shared one reads its whole call log at
# every call, which would grow over 30 minutes into delays the timing would then include.
#
# EVERY ROW OF docs/operator.md's "Every behavior v0.08 adds to this page" TABLE THAT NAMES E0.08-T04-S01 AND
# CONCERNS THE PATCH WAIT, with the cases that show it, each named by its -Case name (a case's results all start with
# the anchor of the operator.md section they enforce, and a case with several results is named with the result's place
# in it). Four rows, in the table's order; the table's other rows name other stories or other files of this story.
#
#  Row 1, `sync`'s patch wait in its place:
#    - Its calls in order, after row 4 and before row 5, with no ec2 describe-instances of its own: patched-at-once
#      (the calls result), pending (the calls result), recent-launch, provision-wait, deploy-wait.
#    - No wait when row 4 fails: install-fails.
#    - Its messages, Checking... and Patched: <id>: patched-at-once (the lines result), recent-launch, deploy-wait.
#    - `pending` then `patched` printing the patching message once: pending (the lines result), old-launch.
#    - The comment `agentspace sync: patch check`, and the question: same-question, patched-at-once (the question result).
#  Row 2, `patched` after a reboot:
#    - The wait through a reboot, each phase message again when it changes, `Patched: <id>` once, rows 5 to 9 run and
#      exit 0: reboot (both results).
#    - Only `patched` and `failed` ending it (unknown, an empty answer, an unexpected answer, an invocation ended in
#      Failed, its output failed included, and 20 polls without an end each keeping it waiting): not-ready-answers, polls.
#    - A host launched more than 20 minutes ago still asked: old-launch, and patched-at-once (a host launched in 2020).
#    - #0034's sign-in failure at `ssm send-command` failing `sync` at once: sign-in; ending `sync -All`: sign-in-all.
#    - A failed `ssm describe-instance-information` failing `sync`: describe-fails; failing one host's part of
#      `sync -All` alone: all (its third result).
#    - `-Quiet` and `sync -All`: quiet; all (its first two results), failed-all.
#  Row 3, a `failed` answer (P21):
#    - After `pending`, and as the first answer, ending at that answer with no further ssm call, the warning exactly
#      once on stderr, the host's alias for a host that is not the box, no `Patched` line, rows 5 to 9 run, exit 0:
#      failed-first (its first and third results), failed-after-pending.
#    - Exit 1 when a later row fails, with that row's message last: failed-later-row.
#    - The warning withheld by `-Quiet`, the exit code the same: failed-first (its second result), failed-later-row
#      (its second result).
#    - `sync -All` not counting the host as failed: failed-all.
#    - `provision`'s step 6 logging `sync done; the patch run failed, see the host's journal` exactly, the host
#      recorded `ready`, exit 0: failed-provision (for a new host, and for a host that was ready).
#    - `up` on a host launched less than 20 minutes ago ending its wait at `failed` with `Ready: <id>` and no other
#      message: same-question (its second result).
#  Row 4, a run that never finishes:
#    - `pending` answered until the 30-minute deadline, its exact message, the host's alias, exit 1, no row 5:
#      deadline (on request).
#    - `provision`'s and `deploy`'s step 6 making the wait's calls: provision-wait and deploy-wait, in the default run,
#      through answers that end the wait at once or after one more check.
#    - A provisioning run whose wait runs out logging `sync failed reason=<id> did not finish its patch run within 30
#      minutes; ...`: deadline-provision (on request).
#
# NOT COVERED, with its reason:
#    - `deploy`'s step 6 when its wait runs out (the deadline): not shown. `deploy`'s step 6 is `sync`, the deadline's
#      message and exit code are shown by `sync` itself (deadline), and a third 30-minute run would show nothing more. The
#      box's `deploy` (no -Host) and `deploy -Host` for a stack it creates are not driven either: their step 6 is the
#      same `sync`; deploy-wait drives `deploy -Host` for a stack that exists through the seam (go and ssh-keygen have no
#      seam, operator.md Q30: PATH holds no go, and the real ssh-keygen signs against the throwaway CA).
#    - The deadline's exact 30 minutes and its failure under `-Quiet` (the deadline's failure always prints): the first
#      is tested to a minute and a half either side, the second would cost a third 30-minute run.
#    - The rows this story shows by another route (the patch check's command text, #0163, #0166, the idle check, the
#      alarm, the guard and its live checks): E0.08-T03-S05's test, the live checks of E0.08-T05-S01 and E0.08-T06, and
#      this story's other files.
#
# What no stand-in shows (the card's live checks, E0.08-T05-S01): that a real host's patch check answers
# `pending` for a failed unit with a start job queued, that the install's `systemctl start --no-block`
# merges into a run under way, and that a patch reboot leaves the wait running. From outside, the host's
# answer is one word, so this file shows what `sync` does with each word and with each other outcome,
# and that the question it sends reads the marker, the unit's state, a queued start job and whether the
# unit is enabled.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes
# every AWS credential. Before any program that can reach AWS starts, the check confirms under the same
# environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run, and reports
# SKIPPED, running nothing, if it does not. Every program the operator can start is a stand-in through
# AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD and AGENTSPACE_SCP_CMD. A failure reason never quotes a program's
# output, and the instance IDs are made up.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-sync-patch-wait.ps1 [-Case <wildcard>|deadline]
param(
  [string]$Case = '*'
)
. (Join-Path $PSScriptRoot 'host-provision-common.ps1')

$ErrorActionPreference = 'Stop'
$pw = 'operator.md#the-patch-wait'
$sy = 'operator.md#sync'
$uw = 'operator.md#what-up-waits-for'
$cb = 'operator.md#calls-by-command'
$af = 'operator.md#aws-failures'
$alias = 'agentspace-host.agentspace'
$hostId = $script:HpHostId
$boxId = $script:HpBoxId
$cmdId = $script:HpPatchWaitCommandId
$msgSsm = 'Waiting for Session Manager...'
$msgPatching = 'Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...'
$deadlineNotRun = 'Not run: the 30-minute deadline case; run it with -Case deadline.'
$deadlineLimitSeconds = 34 * 60
$deadlineMinSeconds = 29 * 60
$deadlineMaxSeconds = 32 * 60
$syncComment = 'agentspace sync: patch check'
$upComment = 'agentspace up: patch check'
function Get-CheckLine([string]$Id) { "Checking that $Id has finished this boot's patch run..." }
function Get-PatchedLine([string]$Id) { "Patched: $Id" }
function Get-FailedWarning([string]$Id, [string]$Alias) { "agentspace: The patch run on $Id failed; sync goes on without it. Check: ssh $Alias journalctl -u agentspace-patch" }
function Get-DeadlineMessage([string]$Id, [string]$Alias) { "agentspace: $Id did not finish its patch run within 30 minutes; check: ssh $Alias journalctl -u agentspace-patch" }
function Get-Selected([string]$Name) { $Name -like $Case }

# ---- The patch wait's answers ---------------------------------------------------------------------------------
function New-Sent { New-HpStep ('{"Command":{"CommandId":"' + $cmdId + '"}}') }
function New-Dii([string]$Ping = 'Online') {
  $list = @(); if ($Ping) { $list = @(@{ PingStatus = $Ping }) }
  New-HpStep (ConvertTo-Json -Compress -Depth 8 -InputObject @{ InstanceInformationList = $list })
}
function New-Invocation([string]$Status, [string]$Output) { New-HpStep (ConvertTo-Json -Compress -InputObject @{ Status = $Status; StandardOutputContent = $Output }) }
function New-Answer([string]$Word) { New-Invocation 'Success' "$Word`n" }
function Get-LaunchTime([double]$MinutesAgo) { [DateTime]::UtcNow.AddMinutes(-$MinutesAgo).ToString("yyyy-MM-dd'T'HH:mm:ss", [Globalization.CultureInfo]::InvariantCulture) + '+00:00' }

# ---- The home ---------------------------------------------------------------------------------------------------
# The box and the host `agentspace`, both ready, with a CLAUDE.md and a fake localswim receipt so that rows 5, 6 and 9
# run, and a scenario whose ssm keys are $Dii, $Send and $Get (one answer each by default, patched at once).
function Set-WaitScenario($H, $S, [object[]]$Dii, [object[]]$Send, [object[]]$Get, [string]$Launch = '2020-01-01T00:00:00+00:00') {
  Set-HpScenario -H $H -S $S -Scenario @{ aws = @{
      'ec2 describe-instances'            = @((New-HpDescribe 'running' $Launch))
      'ssm describe-instance-information' = $Dii
      'ssm send-command'                  = $Send
      'ssm get-command-invocation'        = $Get
    }
  }
}
function Initialize-WaitHome {
  param($H, $S, [object[]]$Dii = @((New-Dii)), [object[]]$Send = @((New-Sent)), [object[]]$Get = @((New-Answer 'patched')), [string]$Launch = '2020-01-01T00:00:00+00:00')
  Initialize-HpHome -H $H -HostsText (Get-HpHostsText -BoxId $boxId -Records @((New-HpRecord 'agentspace' 'ready' $hostId '2026-01-01T00:00:00Z')))
  New-HpClaudeMd $H
  New-HpFakeLocalswim -H $H | Out-Null
  Set-WaitScenario $H $S $Dii $Send $Get $Launch
}
function Invoke-Sync($H, $S, [string[]]$Arguments = @('sync', '-Host', 'agentspace')) { Invoke-HpOperator -H $H -S $S -Arguments $Arguments }

# What a run of `sync` makes before the patch wait (the start and wait of a host launched long ago, rows 1 to 4) and after
# it (rows 5, 6 and 9: the folder, the CLAUDE.md copy and the push; rows 7 and 8 are skipped, with no go on PATH).
$pre = @('aws ec2 describe-instances', 'aws ec2 describe-instances', 'ssh:sync', 'scp', 'scp', 'ssh:sync')
$post = @('ssh:sync', 'scp', 'ssh:push')
$get1 = 'aws ssm get-command-invocation'
$round = @('aws ssm describe-instance-information', 'aws ssm send-command', 'aws ssm get-command-invocation')
$diiOnly = @('aws ssm describe-instance-information')
$diiSend = @('aws ssm describe-instance-information', 'aws ssm send-command')
function Test-SameSeq([string[]]$A, [string[]]$B) { $A.Count -eq $B.Count -and (($A -join [char]0) -ceq ($B -join [char]0)) }
function Add-SequenceProblems($P, $Calls, [string[]]$Want, [string]$What) {
  $got = @(Get-HpSequence $Calls)
  if (Test-SameSeq $got $Want) { return }
  $i = 0
  while ($i -lt $got.Count -and $i -lt $Want.Count -and $got[$i] -ceq $Want[$i]) { $i++ }
  $P.Add("The calls of $What differ at call $($i + 1) of $($Want.Count) (expected $(if ($i -lt $Want.Count) { $Want[$i] } else { 'no more' }), got $(if ($i -lt $got.Count) { $got[$i] } else { 'none' }); $($got.Count) calls were made)")
}
# Every ssm call has the arguments of Calls by command, in its order: a send with the comment given (the nth send takes the
# nth of $Comments, the last repeating), a poll of the command the send answered (the stand-in's command ID of zeros).
function Add-SsmArgumentProblems($P, $Calls, [string]$Id, [string[]]$Comments = @($syncComment)) {
  $want = @{}
  foreach ($w in @(Get-HpPatchWaitCalls $Id)) { $want[[string]$w[1]] = @($w) }
  $n = 0
  $sends = 0
  foreach ($c in @($Calls | Where-Object { $_['kind'] -eq 'aws' -and $_['a'][0] -ceq 'ssm' })) {
    $n++
    $got = @($c['a'])
    $w = @($want[[string]$got[1]])
    if ($w.Count -eq 0) { $P.Add("ssm call $n is an operation of no part of the wait: $($got[1])"); continue }
    # The shared template carries sync's comment; this check names the comment of the nth send itself.
    if ($got[1] -ceq 'send-command') {
      $ci = [Array]::IndexOf($w, '--comment')
      if ($ci -ge 0) { $w[$ci + 1] = $Comments[[Math]::Min($sends, $Comments.Count - 1)] }
      $sends++
    }
    $ok = $got.Count -eq $w.Count
    for ($j = 0; $ok -and $j -lt $w.Count; $j++) {
      if ($w[$j].StartsWith('re:')) { if ($got[$j] -cnotmatch $w[$j].Substring(3)) { $ok = $false } } elseif ($got[$j] -cne $w[$j]) { $ok = $false }
    }
    if (-not $ok) { $P.Add("ssm call $n ($($got[1])) has not the arguments of Calls by command, with the right comment for a send"); return }
  }
}
# The lines of stderr that are the wait's: its four messages and a warning for a failed run.
function Get-WaitLines($Lines, [string]$Id) {
  $set = @((Get-CheckLine $Id), $msgSsm, $msgPatching, (Get-PatchedLine $Id))
  @($Lines | Where-Object { ($set -ccontains $_) -or $_.StartsWith("agentspace: The patch run on $Id failed;", [StringComparison]::Ordinal) })
}
# The seconds between consecutive calls of one name, from the stand-in's ticks.
function Get-Gaps($Calls, [string]$Name) {
  $t = @($Calls | Where-Object { (Get-HpCallName $_) -ceq $Name } | ForEach-Object { [long]$_['t'] })
  for ($i = 1; $i -lt $t.Count; $i++) { ($t[$i] - $t[$i - 1]) / 1e7 }
}
function Get-Question($Calls) {
  $send = @($Calls | Where-Object { $_['kind'] -eq 'aws' -and $_['a'][0] -ceq 'ssm' -and $_['a'][1] -ceq 'send-command' }) | Select-Object -First 1
  if ($null -eq $send) { return $null }
  foreach ($x in @($send['a'])) { if ([string]$x -cmatch '^commands=') { return ([string]$x).Substring(9) } }
  $null
}
function Get-Comment($Call) { $i = [Array]::IndexOf(@($Call['a']), '--comment'); if ($i -ge 0) { [string]$Call['a'][$i + 1] } else { $null } }
# The index of a line in $Lines that starts with $Prefix, or -1.
function Find-LineStarting($Lines, [string]$Prefix) { for ($k = 0; $k -lt $Lines.Count; $k++) { if ($Lines[$k].StartsWith($Prefix, [StringComparison]::Ordinal)) { return $k } }; -1 }

# ---- 1. A host already patched ---------------------------------------------------------------------------------
if (Get-Selected 'patched-at-once') {
  Invoke-HpInHome -Name "${pw}: a host already patched" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking that <id> has finished this boot's patch run... and Patched: <id> ($($wl.Count) lines)") }
    $iCa = -1; for ($k = 0; $k -lt $lines.Count; $k++) { if ($lines[$k] -cmatch '^Copying the agentspace SSH CA''s public key \(SHA256:[A-Za-z0-9+/]+\) to the instance\.\.\.$') { $iCa = $k; break } }
    $iCheck = [Array]::IndexOf($lines, (Get-CheckLine $hostId))
    $iPatched = [Array]::IndexOf($lines, (Get-PatchedLine $hostId))
    $iMd = [Array]::IndexOf($lines, 'Copying ~/.claude/CLAUDE.md to the instance...')
    if (-not ($iCa -ge 0 -and $iCa -lt $iCheck -and $iCheck -lt $iPatched -and $iPatched -lt $iMd)) { $p.Add('The wait''s lines are not between the CA key''s message and Copying ~/.claude/CLAUDE.md') }
    Write-HpProblems "${pw}: a host that answers patched at once prints Checking that <id> has finished this boot's patch run... and Patched: <id>, after the install's row and before row 5, with no Waiting or Patching message and no warning, and sync exits 0 (steps 1 to 3)" $p

    $p = New-HpProblems
    Add-SequenceProblems $p $calls ($pre + $round + $post) 'a sync whose host is already patched'
    Add-SsmArgumentProblems $p $calls $hostId
    Write-HpProblems "${cb}: the wait's calls come after the install (row 4) and before row 5: one ssm describe-instance-information, one ssm send-command with the comment agentspace sync: patch check and one ssm get-command-invocation of the command the send answered, each with the arguments of Calls by command, and no ec2 describe-instances of its own, no ssh and no scp between them (The patch wait, steps 1 and 2)" $p

    $p = New-HpProblems
    $q = Get-Question $calls
    if ($null -eq $q) { $p.Add('No ssm send-command carried a commands= parameter') }
    else {
      if ($q.Length -eq 0) { $p.Add('The question is empty') }
      if (-not $q.Contains('/run/agentspace/patched')) { $p.Add('The question does not name the marker /run/agentspace/patched') }
      if (-not $q.Contains('agentspace-patch')) { $p.Add('The question does not name the patch unit agentspace-patch') }
      if ($q -cnotmatch '(?i)job') { $p.Add('The question does not read whether a start job is queued') }
      if ($q -cnotmatch '(?i)enabled') { $p.Add('The question does not read whether the unit is enabled') }
      foreach ($w in 'patched', 'failed', 'pending', 'unknown') { if ($q -cnotmatch "\b$w\b") { $p.Add("The question does not print the answer $w") } }
    }
    Write-HpProblems "${uw}: the question the patch wait sends names the marker /run/agentspace/patched and the patch unit, reads whether a start job is queued and whether the unit is enabled, and prints one of patched, failed, pending and unknown (the question; P24)" $p
  }
}

# ---- 2. up and sync ask the same question; up does not wait on a failed run -----------------------------------------
if (Get-Selected 'same-question') {
  Invoke-HpInHome -Name "${uw}: the box, sync then up" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s
    $r1 = Invoke-Sync $h $s @('sync')
    $calls1 = @(Get-HpCalls $s)
    $q1 = Get-Question $calls1
    $send1 = @($calls1 | Where-Object { $_['kind'] -eq 'aws' -and $_['a'][1] -ceq 'send-command' }) | Select-Object -First 1
    $p = New-HpProblems
    Add-HpRunProblems $p $r1 0

    # up: a host launched 2 minutes ago, so that its start and wait asks; the host's answer is failed.
    Set-WaitScenario $h $s @((New-Dii)) @((New-Sent)) @((New-Answer 'failed')) (Get-LaunchTime 2)
    $r2 = Invoke-HpOperator -H $h -S $s -Arguments @('up') -Extra @{ AGENTSPACE_NESTED = '1' }
    $calls2 = @(Get-HpCalls $s)
    $q2 = Get-Question $calls2
    $send2 = @($calls2 | Where-Object { $_['kind'] -eq 'aws' -and $_['a'][1] -ceq 'send-command' }) | Select-Object -First 1
    if ($null -eq $q1 -or $null -eq $q2) { $p.Add('A run sent no question') }
    elseif ($q1 -cne $q2) { $p.Add('The question sync''s wait sends is not the question up sends, word for word') }
    if ($null -eq $send1 -or (Get-Comment $send1) -cne $syncComment) { $p.Add('sync''s send-command has not the comment agentspace sync: patch check') }
    if ($null -eq $send2 -or (Get-Comment $send2) -cne $upComment) { $p.Add('up''s send-command has not the comment agentspace up: patch check') }
    Write-HpProblems "${pw}: the wait asks the question up asks, word for word, and Run Command's history tells sync's from up's by the comment alone, agentspace sync: patch check and agentspace up: patch check (step 2; What up waits for)" $p

    $p = New-HpProblems
    Add-HpRunProblems $p $r2 0
    $lines = Get-HpLines $r2.Stderr
    if (-not (Test-SameSeq $lines @("Ready: $boxId"))) { $p.Add('Stderr is not exactly Ready: <id>: up added a message of its own, a Patching message or a wait') }
    if ($r2.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
    Add-SequenceProblems $p $calls2 (@('aws ec2 describe-instances', 'aws ec2 describe-instances') + $round) 'an up of a host whose patch run failed'
    Add-SsmArgumentProblems $p $calls2 $boxId @($upComment)
    Write-HpProblems "${uw}: up does not wait on a failed patch run: a host launched 2 minutes ago that answers failed to the one question is ready at once, with Ready: <id> alone on stderr, nothing on stdout, exit 0, no second check and no ssh or scp (P21, P24)" $p
  }
}

# ---- 3. A pending answer, then patched ------------------------------------------------------------------------------
if (Get-Selected 'pending') {
  Invoke-HpInHome -Name "${pw}: pending, then patched" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s -Get @((New-Answer 'pending'), (New-Answer 'patched'))
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking that <id> has finished this boot's patch run..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
    Write-HpProblems "${pw}: a pending answer is a check that was not ready: Patching Ubuntu (dist-upgrade, reboot if the kernel changed)... is printed once, the wait asks again and ends on patched with Patched: <id>, in that order (step 2)" $p

    $p = New-HpProblems
    Add-SequenceProblems $p $calls ($pre + $round + $round + $post) 'a wait of one pending answer'
    Add-SsmArgumentProblems $p $calls $hostId
    Write-HpProblems "${cb}: a pending check is followed by the whole of the wait's calls again, in order and with the arguments of Calls by command, and rows 5 to 9 follow the second check (The patch wait, step 2)" $p

    $p = New-HpProblems
    $gaps = @(Get-Gaps $calls 'aws ssm describe-instance-information')
    if ($gaps.Count -ne 1) { $p.Add("The agent was checked $($gaps.Count + 1) times, not twice") }
    elseif ($gaps[0] -lt 3) { $p.Add('The two checks were less than 3 seconds apart: the 5-second pause is missing') }
    elseif ($gaps[0] -gt 30) { $p.Add('The two checks were more than 30 seconds apart') }
    Write-HpProblems "${pw}: the wait checks again after a pause of about 5 seconds, never none (step 2; the timing is loose)" $p
  }
}

# ---- 4. -Quiet ----------------------------------------------------------------------------------------------------------
if (Get-Selected 'quiet') {
  Invoke-HpInHome -Name "${pw}: -Quiet" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s -Get @((New-Answer 'pending'), (New-Answer 'patched'))
    $r = Invoke-Sync $h $s @('sync', '-Host', 'agentspace', '-Quiet')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if ($wl.Count -ne 0) { $p.Add("-Quiet left $($wl.Count) of the wait's lines") }
    Add-SequenceProblems $p $calls ($pre + $round + $round + $post) 'a quiet wait of one pending answer'
    Write-HpProblems "${pw}: -Quiet suppresses the wait's first line, its phase messages and its last line, and the wait still goes on until patched (steps 1 and 3; Messages)" $p
  }
}

# ---- 5. Outcomes that are checks that were not ready ----------------------------------------------------------------
if (Get-Selected 'not-ready-answers') {
  Invoke-HpInHome -Name "${pw}: answers that are not ready" -Body {
    param($h, $s)
    $get = @(
      (New-Answer 'unknown'),
      (New-Invocation 'Success' ''),
      (New-Answer 'no such answer'),
      (New-Invocation 'Failed' "patched`n"),
      (New-Invocation 'TimedOut' "failed`n"),
      (New-Answer 'patched'))
    Initialize-WaitHome $h $s -Get $get
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
    $want = $pre
    foreach ($i in 1..6) { $want += $round }
    $want += $post
    Add-SequenceProblems $p $calls $want 'a wait of five answers that are not ready'
    Write-HpProblems "${pw}: unknown, an empty answer, an unexpected answer and an invocation that ended in a status other than Success, one that says patched and one that says failed among them, are checks that were not ready: the wait goes on past all five, prints Patching once, and ends on the sixth, a Success of patched (step 2)" $p
  }
}

# ---- 6. The invocation's polls and the 20-poll rule --------------------------------------------------------------------
if (Get-Selected 'polls') {
  Invoke-HpInHome -Name "${pw}: polls" -Body {
    param($h, $s)
    $get = @(1..20 | ForEach-Object { New-Invocation 'InProgress' '' }) + @(
      (New-HpStep '' 'An error occurred (InvocationDoesNotExist) when calling the GetCommandInvocation operation' 254),
      (New-HpStep '{}'),
      (New-Invocation 'Pending' ''),
      (New-Invocation 'Delayed' ''),
      (New-Answer 'patched'))
    Initialize-WaitHome $h $s -Get $get
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr

    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
    $want = $pre + $diiSend + @(1..20 | ForEach-Object { $get1 }) + $diiSend + @(1..5 | ForEach-Object { $get1 }) + $post
    Add-SequenceProblems $p $calls $want 'a wait whose first check has 20 polls without an end'
    Add-SsmArgumentProblems $p $calls $hostId
    Write-HpProblems "${pw}: 20 polls of the invocation without an end count as pending: the Patching message is printed, the host is asked again at the next check, and the wait ends on patched (step 2; What up waits for, How the answer is read)" $p

    $p = New-HpProblems
    $gets = @($calls | Where-Object { (Get-HpCallName $_) -ceq $get1 })
    if ($gets.Count -ne 25) { $p.Add("The invocation was polled $($gets.Count) times, not 25") }
    else {
      $t = @($gets | ForEach-Object { [long]$_['t'] })
      $gaps = @(1..19 | ForEach-Object { ($t[$_] - $t[$_ - 1]) / 1e7 })
      if (($gaps | Measure-Object -Maximum).Maximum -gt 10) { $p.Add('Two polls of one check were more than 10 seconds apart: the invocation is polled once a second') }
      if (($gaps | Measure-Object -Average).Average -lt 0.9) { $p.Add('The 20 polls of one check were less than a second apart on average') }
    }
    Write-HpProblems "${pw}: the invocation is polled about once a second, at most 20 times to a check, and a failed poll, an answer with no Status and a status of Pending, InProgress or Delayed are polled again, with one send-command to a check (step 2; Calls by command, Which failures are tolerated; the timing is loose)" $p
  }
}

# ---- 7. A reboot in the middle of the wait -------------------------------------------------------------------------------
if (Get-Selected 'reboot') {
  Invoke-HpInHome -Name "${pw}: a reboot" -Body {
    param($h, $s)
    # Six checks: pending; the agent Online but the send refused; no agent (an empty list); the agent ConnectionLost;
    # then pending again, then patched.
    $dii = @((New-Dii), (New-Dii), (New-Dii ''), (New-Dii 'ConnectionLost'), (New-Dii), (New-Dii))
    $send = @((New-Sent), (New-HpStep '' 'An error occurred (InvalidInstanceId) when calling the SendCommand operation: The instance is not in a valid state.' 254), (New-Sent), (New-Sent))
    $get = @((New-Answer 'pending'), (New-Answer 'pending'), (New-Answer 'patched'))
    Initialize-WaitHome $h $s -Dii $dii -Send $send -Get $get
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, $msgSsm, $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., Patching, Waiting for Session Manager... once, Patching again and Patched: <id> ($($wl.Count) lines)") }
    Write-HpProblems "${pw}: a reboot in the middle of the wait, a send-command refused while the agent still reads Online, then an agent that is not Online (an empty list, ConnectionLost), is a run of checks that were not ready: Waiting for Session Manager... is printed once, Patching again after it, and the wait outlasts it and ends on patched (step 2; a message is printed only when it differs from the last one)" $p

    $p = New-HpProblems
    $want = $pre + $round + $diiSend + $diiOnly + $diiOnly + $round + $round + $post
    Add-SequenceProblems $p $calls $want 'a wait through a reboot'
    Write-HpProblems "${cb}: a failed ssm send-command is a check that was not ready, and the wait goes on: it asks the agent again at the next check, asks the host only while the agent is Online, and never fails sync (The patch wait, step 2; Which failures are tolerated)" $p
  }
}

# ---- 8. #0034's sign-in failure of the send ------------------------------------------------------------------------------
if (Get-Selected 'sign-in') {
  Invoke-HpInHome -Name "${af}: the send's sign-in failure" -Body {
    param($h, $s)
    $send = @((New-HpStep '' 'Unable to locate credentials. You can configure credentials by running "aws login".' 255))
    Initialize-WaitHome $h $s -Send $send
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne $script:HpSignIn) { $p.Add('The last line of stderr is not #0034''s sign-in message') }
    Add-SequenceProblems $p $calls ($pre + $diiSend) 'a wait whose send has an expired sign-in'
    if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
    Write-HpProblems "${pw}: a send-command that fails for an expired sign-in is the one failure that is not a check that was not ready: sync fails at once with exactly #0034's message, exit 1, with no later check and no later row (step 2; AWS failures)" $p
  }
}

# ---- 9. describe-instance-information fails ---------------------------------------------------------------------------------
if (Get-Selected 'describe-fails') {
  Invoke-HpInHome -Name "${pw}: describe-instance-information fails" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s
    $forms = @(
      @{ Step = (New-HpStep '' 'An error occurred (AccessDenied) when calling the DescribeInstanceInformation operation: User is not authorized to perform this action' 254); Last = 'agentspace: An error occurred (AccessDenied) when calling the DescribeInstanceInformation operation: User is not authorized to perform this action'
        Name = "a failed ssm describe-instance-information fails sync with agentspace: and the CLI's own error text, exit 1, and nothing after it runs" },
      @{ Step = (New-HpStep '{"InstanceInformationList":[{"InstanceId":"i-fedcba9876543210f"}]}'); Last = 'agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus.'
        Name = "an answer whose first element has no PingStatus fails sync with agentspace: Unexpected answer from ssm describe-instance-information: no InstanceInformationList[0].PingStatus., exit 1, and nothing after it runs" },
      @{ Step = (New-HpStep 'this is not json'); Last = 'agentspace: Unexpected answer from ssm describe-instance-information: not JSON.'
        Name = "an answer that is not JSON fails sync with agentspace: Unexpected answer from ssm describe-instance-information: not JSON., exit 1, and nothing after it runs" },
      @{ Step = (New-HpStep '' 'Your session has expired. Run aws login again.' 255); Last = $script:HpSignIn
        Name = "an expired sign-in fails sync with exactly #0034's message, exit 1, and nothing after it runs" })
    foreach ($f in $forms) {
      Set-WaitScenario $h $s @($f.Step) @((New-Sent)) @((New-Answer 'patched'))
      $r = Invoke-Sync $h $s
      $calls = @(Get-HpCalls $s)
      $lines = Get-HpLines $r.Stderr
      $p = New-HpProblems
      Add-HpRunProblems $p $r 1
      if ($lines.Count -eq 0 -or $lines[-1] -cne $f.Last) { $p.Add('The last line of stderr is not the expected failure message') }
      Add-SequenceProblems $p $calls ($pre + $diiOnly) 'a wait whose first call failed'
      if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
      Write-HpProblems "${af}: in the patch wait, $($f.Name) (The patch wait, step 2; Calls by command; What is read from each answer)" $p
    }
  }
}

# ---- 10. A failed patch run: the wait ends at once with a warning, and sync goes on -------------------------------------
if (Get-Selected 'failed-first') {
  Invoke-HpInHome -Name "${pw}: a failed patch run" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s -Get @((New-Answer 'failed'))
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $warn = Get-FailedWarning $hostId $alias
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $warn))) { $p.Add("The wait's lines are not exactly Checking that <id> has finished this boot's patch run... and the warning, with no Patched and no Patching line ($($wl.Count) lines)") }
    $iWarn = [Array]::IndexOf($lines, $warn)
    $iMd = [Array]::IndexOf($lines, 'Copying ~/.claude/CLAUDE.md to the instance...')
    if (-not ($iWarn -ge 0 -and $iWarn -lt $iMd)) { $p.Add('The warning does not come before Copying ~/.claude/CLAUDE.md to the instance...') }
    Add-SequenceProblems $p $calls ($pre + $round + $post) 'a wait whose first answer is failed'
    Write-HpProblems "${pw}: a failed answer ends the wait at once, at the first check and with no other, with agentspace: The patch run on <id> failed; sync goes on without it. Check: ssh agentspace-host.<project> journalctl -u agentspace-patch, no Patched line, and sync goes on: rows 5 and 6 and the push run and sync exits 0 (steps 2 and 4; P21)" $p

    Set-WaitScenario $h $s @((New-Dii)) @((New-Sent)) @((New-Answer 'failed'))
    $r = Invoke-Sync $h $s @('sync', '-Host', 'agentspace', '-Quiet')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if ($wl.Count -ne 0) { $p.Add("-Quiet left $($wl.Count) of the wait's lines, the warning among them") }
    Add-SequenceProblems $p $calls ($pre + $round + $post) 'a quiet wait whose first answer is failed'
    Write-HpProblems "${pw}: the warning for a failed patch run is a warning, so -Quiet suppresses it, and sync goes on and exits 0 all the same (step 4; Messages)" $p

    Set-WaitScenario $h $s @((New-Dii)) @((New-Sent)) @((New-Answer 'failed'))
    $r = Invoke-Sync $h $s @('sync')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $boxId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $boxId), (Get-FailedWarning $boxId 'agentspace')))) { $p.Add('The wait''s lines are not Checking... and the warning naming ssh agentspace journalctl -u agentspace-patch, the box''s alias') }
    Add-SequenceProblems $p $calls ($pre + $round + $post) 'a box sync whose first answer is failed'
    Write-HpProblems "${pw}: for the box the warning's check command is ssh agentspace journalctl -u agentspace-patch, and for a host that is not the box it names the host's alias (step 4)" $p
  }
}

# ---- 11. A failed run after a pending answer: a stale failure is not this run's -----------------------------------------
if (Get-Selected 'failed-after-pending') {
  Invoke-HpInHome -Name "${pw}: pending, then failed" -Body {
    param($h, $s)
    # A failed unit with a start job queued answers pending (P24): the check that runs between the install's start and
    # the run's start never reads the earlier run's failure. The run then fails, and the next answer is failed.
    Initialize-WaitHome $h $s -Get @((New-Answer 'pending'), (New-Answer 'failed'))
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-FailedWarning $hostId $alias)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message and the warning ($($wl.Count) lines)") }
    Add-SequenceProblems $p $calls ($pre + $round + $round + $post) 'a wait of one pending answer and then failed'
    Write-HpProblems "${pw}: a pending answer, which is what a failed unit with a start job queued gives (P24), does not end the wait, and the first failed answer after it does, with the warning and no further check, and sync goes on and exits 0 (steps 2 and 4; P21)" $p
  }
}

# ---- 12. -All: a failed run is not a failed host --------------------------------------------------------------------------
if (Get-Selected 'failed-all') {
  Invoke-HpInHome -Name "${pw}: sync -All" -Body {
    param($h, $s)
    # The box is patched, the host's run failed: each host's part has its own wait.
    Initialize-WaitHome $h $s -Get @((New-Answer 'patched'), (New-Answer 'failed'))
    $r = Invoke-Sync $h $s @('sync', '-All')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { $p.Add('A closing count was printed: a failed patch run is not a failed host') }
    $wl = @(Get-WaitLines $lines $boxId)
    $wl2 = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $boxId), (Get-PatchedLine $boxId)))) { $p.Add('The box''s part has not exactly Checking... and Patched: <id>') }
    if (-not (Test-SameSeq $wl2 @((Get-CheckLine $hostId), (Get-FailedWarning $hostId $alias)))) { $p.Add('The host''s part has not exactly Checking... and the warning naming its own alias') }
    Add-SequenceProblems $p $calls (($pre + $round + $post) + ($pre + $round + $post)) 'sync -All over the box and a host whose patch run failed'
    Write-HpProblems "${pw}: with -All each host waits in its own part, and a host whose patch run failed is not a failed part: its warning names its own alias, it is not counted, no closing count is printed and the command exits 0 (The patch wait, step 4; Every host at once)" $p
  }
}

# ---- 13. A host launched recently --------------------------------------------------------------------------------------------
if (Get-Selected 'recent-launch') {
  Invoke-HpInHome -Name "${pw}: a recent launch" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s -Launch (Get-LaunchTime 2)
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    $want = @('aws ec2 describe-instances', 'aws ec2 describe-instances') + $round + @('ssh:sync', 'scp', 'scp', 'ssh:sync') + $round + $post
    Add-SequenceProblems $p $calls $want 'a sync of a host launched 2 minutes ago'
    Add-SsmArgumentProblems $p $calls $hostId @($upComment, $syncComment)
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), (Get-PatchedLine $hostId)))) { $p.Add('The patch wait''s lines are not exactly Checking... and Patched: <id>') }
    if (-not ($lines -ccontains "Ready: $hostId")) { $p.Add('The start and wait did not print Ready: <id>') }
    Write-HpProblems "${pw}: a host launched 2 minutes ago is asked by the start and wait before the install, with up's comment, and asked again by the patch wait after it, with sync's, which reads no launch time of its own: no ec2 describe-instances after the install (steps 1 and 2; Starting the box and waiting for it)" $p
  }
}

# ---- 13a. No wait when the install fails -------------------------------------------------------------------------------------
if (Get-Selected 'install-fails') {
  Invoke-HpInHome -Name "${pw}: the install fails" -Body {
    param($h, $s)
    Initialize-WaitHome $h $s
    # Row 4, the install, is the second ssh of a sync.
    Set-HpScenario -H $h -S $s -Scenario @{ rules = @(@{ kind = 'ssh'; class = 'sync'; nth = 2; code = 1; err = "install wrote this`n" }); aws = @{
        'ec2 describe-instances'            = @((New-HpDescribe 'running' '2020-01-01T00:00:00+00:00'))
        'ssm describe-instance-information' = @((New-Dii))
        'ssm send-command'                  = @((New-Sent))
        'ssm get-command-invocation'        = @((New-Answer 'patched'))
      }
    }
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne 'agentspace: Install on the instance failed.') { $p.Add('The last line of stderr is not agentspace: Install on the instance failed.') }
    Add-SequenceProblems $p $calls $pre 'a sync whose install fails'
    if (@(Get-WaitLines $lines $hostId).Count -ne 0) { $p.Add('A line of the patch wait was printed') }
    Write-HpProblems "${pw}: when row 4, the install, fails there is no patch wait: sync fails with the install's message, exit 1, and no ssm call and no row after it is made (The patch wait: it runs only when row 4 exited 0)" $p
  }
}

# ---- 13b. A host launched more than 20 minutes ago is asked all the same --------------------------------------------------
if (Get-Selected 'old-launch') {
  Invoke-HpInHome -Name "${pw}: an old launch" -Body {
    param($h, $s)
    # Launched 30 minutes ago, so that the start and wait before the install asks nothing (the 20-minute rule), and the
    # host answers unknown, one of the four outcomes `up`'s wait counts as finished, and then patched.
    Initialize-WaitHome $h $s -Get @((New-Answer 'unknown'), (New-Answer 'patched')) -Launch (Get-LaunchTime 30)
    $r = Invoke-Sync $h $s
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    Add-SequenceProblems $p $calls ($pre + $round + $round + $post) 'a sync of a host launched 30 minutes ago whose first answer is unknown'
    Add-SsmArgumentProblems $p $calls $hostId
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
    if (-not ($lines -ccontains "Ready: $hostId")) { $p.Add('The start and wait did not print Ready: <id>, which it prints at its end whether it asked or not (Starting the box, step 4)') }
    Write-HpProblems "${pw}: a host launched more than 20 minutes ago is still asked by the patch wait, once at each check, with none of the four exceptions of up's wait: unknown is a check that was not ready, and the wait ends on the second answer, patched (step 2; Starting the box and waiting for it)" $p
  }
}

# ---- 13c. A failed run, then a later row that fails ---------------------------------------------------------------------------
if (Get-Selected 'failed-later-row') {
  Invoke-HpInHome -Name "${pw}: a failed patch run, then a failing row" -Body {
    param($h, $s)
    # Row 6, the CLAUDE.md copy, is the third scp of a sync.
    $rules = @(@{ kind = 'scp'; nth = 3; code = 1; err = "scp wrote this`n" })
    $failedRow = 'agentspace: Copying CLAUDE.md to the instance failed.'
    $warn = Get-FailedWarning $hostId $alias
    foreach ($form in @(@{ Args = @('sync', '-Host', 'agentspace'); Quiet = $false }, @{ Args = @('sync', '-Host', 'agentspace', '-Quiet'); Quiet = $true })) {
      Initialize-WaitHome $h $s -Get @((New-Answer 'failed'))
      Set-HpScenario -H $h -S $s -Scenario @{ rules = $rules; aws = @{
          'ec2 describe-instances'            = @((New-HpDescribe 'running' '2020-01-01T00:00:00+00:00'))
          'ssm describe-instance-information' = @((New-Dii))
          'ssm send-command'                  = @((New-Sent))
          'ssm get-command-invocation'        = @((New-Answer 'failed'))
        }
      }
      $r = Invoke-Sync $h $s $form.Args
      $calls = @(Get-HpCalls $s)
      $lines = Get-HpLines $r.Stderr
      $p = New-HpProblems
      Add-HpRunProblems $p $r 1
      if ($lines.Count -eq 0 -or $lines[-1] -cne $failedRow) { $p.Add('The last line of stderr is not the failing row''s own message, agentspace: Copying CLAUDE.md to the instance failed.') }
      $iWarn = [Array]::IndexOf($lines, $warn)
      if ($form.Quiet) { if ($iWarn -ge 0) { $p.Add('-Quiet did not withhold the warning') } }
      elseif ($iWarn -lt 0) { $p.Add('The warning for the failed patch run was not printed') }
      elseif ($iWarn -ge $lines.Count - 1) { $p.Add('The warning does not come before the failing row''s message') }
      if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
      Add-SequenceProblems $p $calls ($pre + $round + @('ssh:sync', 'scp')) 'a sync whose patch run failed and whose row 6 fails'
      $name = if ($form.Quiet) { 'with -Quiet the warning is withheld, the exit code is the same and the failing row''s message is still last' } else { 'the warning is printed, and then the failing row makes sync exit 1 with that row''s message last, so that a failed patch run does not hide a later failure' }
      Write-HpProblems "${pw}: after a failed answer, $name (step 4; P21)" $p
    }
  }
}

# ---- 13d. provision's step 6 ------------------------------------------------------------------------------------------------
# A provisioning run's calls for a host with a CLAUDE.md and a fake receipt, so that rows 5, 6 and 9 run, and no go on PATH:
# the existence check, the deploy, the instance ID read, the two start and waits' describe-instances, the sync, and bootstrap.
function Get-ProvisionWant([string[]]$Wait) {
  @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks') + @(1..4 | ForEach-Object { 'aws ec2 describe-instances' }) +
  @('ssh:sync', 'scp', 'scp', 'ssh:sync') + $Wait + @('ssh:sync', 'scp', 'ssh:push') + @(1..2 | ForEach-Object { 'aws ec2 describe-instances' }) + @('ssh:bootstrap')
}
$pv = 'operator.md#provision'
$provisionSteps = 'run begun,deploy done,start done,sync done,bootstrap done,record done,place done placed=0,run done'
$signInLine = 'Sign in on this host once, by hand, under a lease: see "Signing in on a new host" in the agentspace README.'
function Initialize-ProvisionHome {
  param($H, $S, [object[]]$Get, [switch]$Ready, [string]$Launch = '2020-01-01T00:00:00+00:00')
  $records = @()
  if ($Ready) { $records = @((New-HpRecord 'agentspace' 'ready' $hostId '2026-01-01T00:00:00Z')) }
  Initialize-HpHome -H $H -HostsText (Get-HpHostsText -BoxId $boxId -Records $records)
  New-HpClaudeMd $H
  New-HpFakeLocalswim -H $H | Out-Null
  $aws = Get-HpHostAws -Project 'agentspace' -Id $hostId -Exists:$Ready
  $aws['ec2 describe-instances'] = @((New-HpDescribe 'running' $Launch))
  $aws['ssm describe-instance-information'] = @((New-Dii))
  $aws['ssm send-command'] = @((New-Sent))
  $aws['ssm get-command-invocation'] = $Get
  Set-HpScenario -H $H -S $S -LockProject 'agentspace' -Scenario @{ aws = $aws; rules = @((Get-HpBootstrapRule)) }
}
if (Get-Selected 'provision-wait') {
  Invoke-HpInHome -Name "${pw}: provision's step 6" -Body {
    param($h, $s)
    Initialize-ProvisionHome $h $s -Get @((New-Answer 'pending'), (New-Answer 'patched'))
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $log = Read-HpLog $h 'agentspace'
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    Add-SequenceProblems $p $calls (Get-ProvisionWant ($round + $round)) 'a provisioning run whose host answers pending and then patched'
    Add-SsmArgumentProblems $p $calls $hostId
    $wl = @(Get-WaitLines $lines $hostId)
    if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
    $iInst = [Array]::IndexOf($lines, "Instance: $hostId")
    $iCheck = [Array]::IndexOf($lines, (Get-CheckLine $hostId))
    $iBoot = [Array]::IndexOf($lines, "Bootstrapped: $hostId")
    if (-not ($iInst -ge 0 -and $iInst -lt $iCheck -and $iCheck -lt $iBoot)) { $p.Add('The wait''s lines are not between the deploy step''s Instance: <id> and bootstrap''s Bootstrapped: <id>') }
    if ((Get-HpLogShape $log) -cne $provisionSteps) { $p.Add("The log's lines are not: $provisionSteps") }
    $syncLines = @($log.Lines | Where-Object { $_.Step -eq 'sync' })
    if ($syncLines.Count -ne 1 -or $syncLines[0].Rest -cne '') { $p.Add('The sync line is not <time> sync done alone') }
    Write-HpProblems "${pw}: provision's step 6 is sync, its patch wait included: the wait's three Session Manager calls come after the install's row and before row 5, the host asked again after a pending answer, the messages between Instance: <id> and Bootstrapped: <id>, and a patched host logs sync done with no other field (provision, step 6; The patch wait)" $p
  }
}
if (Get-Selected 'failed-provision') {
  foreach ($ready in $false, $true) {
    $label = if ($ready) { 'a host that was ready' } else { 'a new host' }
    Invoke-HpInHome -Name "${pv}: a failed patch run, $label" -Body {
      param($h, $s)
      Initialize-ProvisionHome $h $s -Get @((New-Answer 'pending'), (New-Answer 'failed')) -Ready:$ready
      $t0 = [DateTime]::UtcNow
      $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace')
      $t1 = [DateTime]::UtcNow
      $calls = @(Get-HpCalls $s)
      $lines = Get-HpLines $r.Stderr
      $log = Read-HpLog $h 'agentspace'
      $p = New-HpProblems
      Add-HpRunProblems $p $r 0
      Add-SequenceProblems $p $calls (Get-ProvisionWant ($round + $round)) 'a provisioning run whose host answers pending and then failed'
      if ((Get-HpLogShape $log) -cne $provisionSteps) { $p.Add("The log's lines are not: $provisionSteps") }
      $syncLines = @($log.Lines | Where-Object { $_.Step -eq 'sync' })
      if ($syncLines.Count -ne 1) { $p.Add("The log holds $($syncLines.Count) sync lines, not one") }
      elseif ($syncLines[0].Raw -cnotmatch '^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ sync done; the patch run failed, see the host''s journal$') { $p.Add('The sync line is not exactly <time> sync done; the patch run failed, see the host''s journal') }
      elseif (-not (Test-HpTimeBetween $syncLines[0].Time $t0 $t1)) { $p.Add('The sync line''s time is not the time it was written') }
      foreach ($l in @($log.Lines | Where-Object { $_.Step -ne 'sync' -and $_.Outcome -eq 'done' -and $_.Step -ne 'place' -and $_.Rest -ne '' })) { $p.Add("The $($l.Step) line holds text after its outcome: only the sync line of a failed patch run does"); break }
      $fin = Get-HpRecord $h 'agentspace'
      if ($null -eq $fin -or $fin['state'] -cne 'ready' -or $fin['instance_id'] -cne $hostId) { $p.Add('The host is not recorded ready with its instance ID') }
      if (@($lines | Where-Object { $_ -ceq (Get-FailedWarning $hostId $alias) }).Count -ne 1) { $p.Add('The warning for the failed patch run was not printed exactly once, naming the host''s alias') }
      if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
      if (-not ($lines -ccontains "Bootstrapped: $hostId")) { $p.Add('The run did not go on to bootstrap') }
      if ($ready) { if ($lines | Where-Object { $_ -like 'Sign in on this host*' }) { $p.Add('The sign-in line was printed for a host that was ready') } }
      elseif ($lines.Count -eq 0 -or $lines[-1] -cne $signInLine) { $p.Add('The last line of stderr is not the sign-in line') }
      Write-HpProblems "${pv}: a failed patch run does not fail provision: step 6 logs sync done; the patch run failed, see the host's journal exactly, with no other field, the run goes on to bootstrap, logs the rest of its steps, records the host ready and exits 0, for $label (step 6; its log; P21, P27)" $p
    }
  }
}

# ---- 13e. deploy's step 6 ---------------------------------------------------------------------------------------------------
if (Get-Selected 'deploy-wait') {
  Invoke-HpInHome -Name 'operator.md#deploy: step 6' -Body {
    param($h, $s)
    # deploy -Host on a host whose stack exists: the seam covers every program the run starts but go and ssh-keygen
    # (operator.md, Q30), PATH holds no go, and the real ssh-keygen makes the host's key against the throwaway CA, as for sync.
    $post = @('ssh:sync', 'scp', 'ssh:push')
    $want = @('aws cloudformation describe-stacks', 'aws cloudformation deploy', 'aws cloudformation describe-stacks', 'aws ec2 describe-instances', 'aws ec2 describe-instances', 'ssh:sync', 'scp', 'scp', 'ssh:sync')
    foreach ($form in @(@{ Answers = @((New-Answer 'pending'), (New-Answer 'patched')); Wait = ($round + $round); Failed = $false }, @{ Answers = @((New-Answer 'failed')); Wait = $round; Failed = $true })) {
      Initialize-ProvisionHome $h $s -Get $form.Answers -Ready
      $r = Invoke-HpOperator -H $h -S $s -Arguments @('deploy', '-Host', 'agentspace')
      $calls = @(Get-HpCalls $s)
      $lines = Get-HpLines $r.Stderr
      $p = New-HpProblems
      Add-HpRunProblems $p $r 0
      Add-SequenceProblems $p $calls ($want + $form.Wait + $post) 'a deploy whose sync waits'
      Add-SsmArgumentProblems $p $calls $hostId
      $iInst = [Array]::IndexOf($lines, "Instance: $hostId")
      $iCheck = [Array]::IndexOf($lines, (Get-CheckLine $hostId))
      $wl = @(Get-WaitLines $lines $hostId)
      if ($form.Failed) {
        if (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), (Get-FailedWarning $hostId $alias)))) { $p.Add("The wait's lines are not exactly Checking... and the warning ($($wl.Count) lines)") }
      } elseif (-not (Test-SameSeq $wl @((Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId)))) { $p.Add("The wait's lines are not exactly Checking..., the Patching message once and Patched: <id> ($($wl.Count) lines)") }
      if (-not ($iInst -ge 0 -and $iInst -lt $iCheck)) { $p.Add('The wait''s first line does not come after the deploy step''s Instance: <id>') }
      $what = if ($form.Failed) { 'a failed answer ends the wait at once with the warning and deploy exits 0' } else { 'a pending answer and then patched, and deploy exits 0' }
      Write-HpProblems "operator.md#deploy: step 6 is sync, its patch wait included: after the deploy step's own calls the wait's three Session Manager calls come after the install's row and before row 5, and $what (step 6; The patch wait)" $p
    }
  }
}

# ---- 13f. -All: each host waits in its own part --------------------------------------------------------------------------------
$boxHostLine = 'Host      swimcoach (agentbox), ready'
$hostHostLine = 'Host      agentspace (agentspace-host-agentspace), ready'
function Get-AllWait($Lines) {
  $set = @((Get-CheckLine $boxId), (Get-PatchedLine $boxId), (Get-CheckLine $hostId), (Get-PatchedLine $hostId), $msgSsm, $msgPatching)
  @($Lines | Where-Object { ($set -ccontains $_) -or $_.StartsWith('agentspace: The patch run on i-', [StringComparison]::Ordinal) })
}
if (Get-Selected 'all') {
  Invoke-HpInHome -Name "${pw}: sync -All" -Body {
    param($h, $s)
    $get = @((New-Answer 'pending'), (New-Answer 'patched'), (New-Answer 'pending'), (New-Answer 'patched'))
    Initialize-WaitHome $h $s -Get $get
    $r = Invoke-Sync $h $s @('sync', '-All')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    Add-SequenceProblems $p $calls (($pre + $round + $round + $post) + ($pre + $round + $round + $post)) 'sync -All over the box and a host, each pending once'
    $all = @(Get-AllWait $lines)
    $wantAll = @((Get-CheckLine $boxId), $msgPatching, (Get-PatchedLine $boxId), (Get-CheckLine $hostId), $msgPatching, (Get-PatchedLine $hostId))
    if (-not (Test-SameSeq $all $wantAll)) { $p.Add("The wait's lines are not each host's own: Checking, Patching once and Patched for the box, then the same for the host ($($all.Count) lines)") }
    $iBox = [Array]::IndexOf($lines, $boxHostLine)
    $iHost = [Array]::IndexOf($lines, $hostHostLine)
    $iBoxPatched = [Array]::IndexOf($lines, (Get-PatchedLine $boxId))
    $iHostCheck = [Array]::IndexOf($lines, (Get-CheckLine $hostId))
    if (-not ($iBox -ge 0 -and $iBox -lt $iBoxPatched -and $iBoxPatched -lt $iHost -and $iHost -lt $iHostCheck)) { $p.Add('The Host lines do not bracket each host''s own wait') }
    if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { $p.Add('A closing count was printed though no host failed') }
    Write-HpProblems "${pw}: with -All each host waits in its own part, with its own messages, its Patching message printed once in each, and the Host line before each part, exit 0 and no closing count (The patch wait; Every host at once)" $p

    Set-WaitScenario $h $s @((New-Dii)) @((New-Sent)) $get
    $r = Invoke-Sync $h $s @('sync', '-All', '-Quiet')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 0
    if (@(Get-AllWait $lines).Count -ne 0) { $p.Add('-Quiet left lines of the wait on stderr') }
    if (-not ($lines -ccontains $boxHostLine -and $lines -ccontains $hostHostLine)) { $p.Add('-Quiet withheld a Host line, which prints whatever -Quiet says') }
    Add-SequenceProblems $p $calls (($pre + $round + $round + $post) + ($pre + $round + $round + $post)) 'a quiet sync -All'
    Write-HpProblems "${pw}: sync -All -Quiet suppresses the wait's messages in every part and still waits in each, and the Host lines print all the same (Messages; Every host at once)" $p

    # The box's agent check fails: its part fails, the host's part goes on, and the count is printed.
    $accessDenied = 'An error occurred (AccessDenied) when calling the DescribeInstanceInformation operation: User is not authorized to perform this action'
    Set-WaitScenario $h $s @((New-HpStep '' $accessDenied 254), (New-Dii)) @((New-Sent)) @((New-Answer 'patched'))
    $r = Invoke-Sync $h $s @('sync', '-All')
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne 'agentspace: 1 of 2 hosts failed.') { $p.Add('The last line of stderr is not agentspace: 1 of 2 hosts failed.') }
    if (-not ($lines -ccontains "agentspace: $accessDenied")) { $p.Add('The box''s part did not print the failure of its ssm describe-instance-information') }
    if (-not ($lines -ccontains (Get-PatchedLine $hostId))) { $p.Add('The host''s part did not go on to its own wait and end it') }
    Add-SequenceProblems $p $calls (($pre + $diiOnly) + ($pre + $round + $post)) 'sync -All whose first host fails at its wait'
    Write-HpProblems "${af}: with -All a failed ssm describe-instance-information fails that host's part alone: the next host is waited for and synced, the count is agentspace: 1 of 2 hosts failed., exit 1 (The patch wait, step 2; Every host at once)" $p
  }
}
if (Get-Selected 'sign-in-all') {
  Invoke-HpInHome -Name "${af}: sync -All and the send's sign-in failure" -Body {
    param($h, $s)
    $signInErr = 'Unable to locate credentials. You can configure credentials by running "aws login".'
    # The sign-in failure is the first host's, and then the second's: either ends the command at once.
    $forms = @(
      @{ Send = @((New-HpStep '' $signInErr 255)); Want = ($pre + $diiSend); HostsTouched = $false; Name = 'at the first host''s send, no later host is touched' },
      @{ Send = @((New-Sent), (New-HpStep '' $signInErr 255)); Want = (($pre + $round + $post) + ($pre + $diiSend)); HostsTouched = $true; Name = 'at the second host''s send, the first host''s part has finished and nothing runs after' })
    foreach ($f in $forms) {
      Initialize-WaitHome $h $s -Send $f.Send
      $r = Invoke-Sync $h $s @('sync', '-All')
      $calls = @(Get-HpCalls $s)
      $lines = Get-HpLines $r.Stderr
      $p = New-HpProblems
      Add-HpRunProblems $p $r 1
      if ($lines.Count -eq 0 -or $lines[-1] -cne $script:HpSignIn) { $p.Add('The last line of stderr is not #0034''s sign-in message') }
      if ($lines | Where-Object { $_ -match '^agentspace: \d+ of \d+ hosts failed\.$' }) { $p.Add('A closing count was printed: the sign-in failure ends the command at once') }
      if (-not $f.HostsTouched -and ($lines -ccontains $hostHostLine -or $lines -ccontains (Get-CheckLine $hostId))) { $p.Add('The command went on to the second host after the sign-in failure') }
      Add-SequenceProblems $p $calls $f.Want 'sync -All whose send has an expired sign-in'
      Write-HpProblems "${af}: #0034's sign-in failure of the patch wait's send-command ends sync -All at once, with exactly its message, exit 1 and no closing count: $($f.Name) (The patch wait, step 2; Every host at once)" $p
    }
  }
}

# ---- 14. The 30-minute deadline, waited in full -----------------------------------------------------------------------------
# Runs only with -Case deadline, exactly: otherwise it writes no result, and a -Case that would select it prints the one
# Not-run line (docs/conformance.md, Cases that run only when asked for).
$script:LeanAwsScript = @'
$dir = $PSScriptRoot
$key = [string]$args[0] + ' ' + [string]$args[1]
$line = ConvertTo-Json -Compress -InputObject @{ t = [string][DateTime]::UtcNow.Ticks; a = @($args | ForEach-Object { [string]$_ }) }
[IO.File]::AppendAllText((Join-Path $dir 'lean-aws.log'), ($line + "`n"))
switch ($key) {
  'ec2 describe-instances' { [Console]::Out.Write('{"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}') }
  'ssm describe-instance-information' { [Console]::Out.Write('{"InstanceInformationList":[{"PingStatus":"Online"}]}') }
  'ssm send-command' { [Console]::Out.Write('{"Command":{"CommandId":"00000000-0000-0000-0000-000000000000"}}') }
  'ssm get-command-invocation' { [Console]::Out.Write('{"Status":"Success","StandardOutputContent":"pending\n"}') }
}
exit 0
'@
if ((Get-Selected 'deadline') -and $Case -ine 'deadline') {
  Write-Host $deadlineNotRun
} elseif (Get-Selected 'deadline') {
  Invoke-HpInHome -Name "${pw}: the 30-minute deadline" -Body {
    param($h, $s)
    # Online, and the host answers pending to every question: a patch run that never finishes.
    Initialize-WaitHome $h $s
    Write-HpFile -H $h -Path (Join-Path $s.Dir 'lean-aws.ps1') -Text $script:LeanAwsScript
    $lean = New-OperatorStandIn -ConformanceHome $h -Name 'aws-lean' -Body @(('"' + $script:HpPwsh + '" -NoProfile -File "%~dp0lean-aws.ps1" %*'), 'exit /b %ERRORLEVEL%')
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('sync', '-Host', 'agentspace') -Extra @{ AGENTSPACE_AWS_CMD = $lean } -Timeout $deadlineLimitSeconds
    $clock.Stop()
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $leanLog = Join-Path $s.Dir 'lean-aws.log'
    $aws = @()
    if (Test-Path -LiteralPath $leanLog -PathType Leaf) { $aws = @(Get-Content -LiteralPath $leanLog | Where-Object { $_.Trim() -ne '' } | ForEach-Object { ConvertFrom-Json -InputObject $_ -AsHashtable }) }

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne (Get-DeadlineMessage $hostId $alias)) { $p.Add('The last line of stderr is not agentspace: <id> did not finish its patch run within 30 minutes; check: ssh agentspace-host.<project> journalctl -u agentspace-patch') }
    if (-not (Test-SameSeq @(Get-HpSequence $calls) @('ssh:sync', 'scp', 'scp', 'ssh:sync'))) { $p.Add('An ssh or scp ran after the install: rows 5 to 9 are not run when the wait runs out') }
    if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
    Write-HpProblems "${pw}: a host whose patch run never finishes fails sync at the deadline with exactly agentspace: <id> did not finish its patch run within 30 minutes; check: ssh agentspace-host.<project> journalctl -u agentspace-patch, exit 1, and rows 5 to 9 are not run (step 2)" $p

    $p = New-HpProblems
    if ($r.TimedOut) { $p.Add('It was still running after 34 minutes') }
    elseif ($clock.Elapsed.TotalSeconds -lt $deadlineMinSeconds) { $p.Add('It gave up in less than 29 minutes') }
    elseif ($clock.Elapsed.TotalSeconds -gt $deadlineMaxSeconds) { $p.Add('It gave up after more than 32 minutes') }
    Write-HpProblems "${pw}: the deadline is 30 minutes from the start of the wait, with the last pause cut short to end on it, not the 15 minutes of up's wait (step 2; P13; the timing is loose)" $p

    $p = New-HpProblems
    $ops = @($aws | ForEach-Object { $_['a'][0] + ' ' + $_['a'][1] })
    $nDii = @($ops | Where-Object { $_ -ceq 'ssm describe-instance-information' }).Count
    if ($nDii -lt 150) { $p.Add("The agent was checked only $nDii times in 30 minutes, so the poll did not repeat about every 5 seconds") }
    if (@($ops | Where-Object { $_ -cnotin 'ec2 describe-instances', 'ssm describe-instance-information', 'ssm send-command', 'ssm get-command-invocation' }).Count -gt 0) { $p.Add('A call was made that is no part of the start and wait or the patch wait') }
    if (@($ops | Where-Object { $_ -ceq 'ec2 describe-instances' }).Count -ne 2) { $p.Add('ec2 describe-instances was called other than twice: the wait reads no launch time') }
    $nPatching = @($lines | Where-Object { $_ -ceq $msgPatching }).Count
    if ($nPatching -ne 1) { $p.Add("The Patching message was printed $nPatching times, not once, over the whole wait") }
    Write-HpProblems "${pw}: over the 30 minutes the wait repeats, makes no call but the patch wait's, and prints the Patching message once, not at every check (step 2)" $p
  }
}

# ---- 15. A provisioning run whose wait runs out, waited in full ----------------------------------------------------------------
# A second on-request case, run only with -Case deadline-provision, exactly, under the same rules as `deadline`: a new host
# whose provisioning run reaches step 6, whose host answers pending to every question for the 30 minutes, and whose run then
# logs `sync failed reason=<message>`. Its stand-in `aws` is lean, as the deadline case's, and also answers the deploy step.
$script:LeanProvisionAwsScript = @'
$dir = $PSScriptRoot
$key = [string]$args[0] + ' ' + [string]$args[1]
$logFile = Join-Path $dir 'lean-aws.log'
$prior = 0
if ($key -eq 'cloudformation describe-stacks' -and (Test-Path -LiteralPath $logFile)) { $prior = @(Get-Content -LiteralPath $logFile | Where-Object { $_ -like '*"cloudformation","describe-stacks"*' }).Count }
$line = ConvertTo-Json -Compress -InputObject @{ t = [string][DateTime]::UtcNow.Ticks; a = @($args | ForEach-Object { [string]$_ }) }
[IO.File]::AppendAllText($logFile, ($line + "`n"))
switch ($key) {
  'cloudformation describe-stacks' {
    if ($prior -eq 0) { [Console]::Error.Write('An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id agentspace-host-agentspace does not exist'); exit 254 }
    [Console]::Out.Write('{"Stacks":[{"StackName":"agentspace-host-agentspace","StackStatus":"CREATE_COMPLETE","Outputs":[{"OutputKey":"InstanceId","OutputValue":"__HOSTID__"}]}]}')
  }
  'cloudformation deploy' { [Console]::Out.Write("deploy-out`n") }
  'ec2 describe-instances' { [Console]::Out.Write('{"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}') }
  'ssm describe-instance-information' { [Console]::Out.Write('{"InstanceInformationList":[{"PingStatus":"Online"}]}') }
  'ssm send-command' { [Console]::Out.Write('{"Command":{"CommandId":"00000000-0000-0000-0000-000000000000"}}') }
  'ssm get-command-invocation' { [Console]::Out.Write('{"Status":"Success","StandardOutputContent":"pending\n"}') }
}
exit 0
'@
$deadlineProvisionNotRun = 'Not run: the 30-minute provisioning deadline case; run it with -Case deadline-provision.'
if ((Get-Selected 'deadline-provision') -and $Case -ine 'deadline-provision') {
  Write-Host $deadlineProvisionNotRun
} elseif (Get-Selected 'deadline-provision') {
  Invoke-HpInHome -Name "${pv}: the 30-minute deadline" -Body {
    param($h, $s)
    $leaseId = '11111111-1111-4111-8111-111111111111'
    Initialize-ProvisionHome $h $s -Get @((New-Answer 'patched'))
    Set-HpLeases $h @((New-HpLeaseRecord -Id $leaseId))
    $leasesBefore = Get-HpFileBytes (Get-HpLeasesFile $h)
    Write-HpFile -H $h -Path (Join-Path $s.Dir 'lean-aws.ps1') -Text $script:LeanProvisionAwsScript.Replace('__HOSTID__', $hostId)
    $lean = New-OperatorStandIn -ConformanceHome $h -Name 'aws-lean' -Body @(('"' + $script:HpPwsh + '" -NoProfile -File "%~dp0lean-aws.ps1" %*'), 'exit /b %ERRORLEVEL%')
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $r = Invoke-HpOperator -H $h -S $s -Arguments @('provision', '-Host', 'agentspace') -Extra @{ AGENTSPACE_AWS_CMD = $lean } -Timeout (35 * 60)
    $clock.Stop()
    $calls = @(Get-HpCalls $s)
    $lines = Get-HpLines $r.Stderr
    $log = Read-HpLog $h 'agentspace'
    $msg = Get-DeadlineMessage $hostId $alias

    $p = New-HpProblems
    Add-HpRunProblems $p $r 1
    if ($lines.Count -eq 0 -or $lines[-1] -cne $msg) { $p.Add('The last line of stderr is not agentspace: <id> did not finish its patch run within 30 minutes; check: ssh agentspace-host.<project> journalctl -u agentspace-patch') }
    if (-not (Test-SameSeq @(Get-HpSequence $calls) @('ssh:sync', 'scp', 'scp', 'ssh:sync'))) { $p.Add('An ssh or scp ran after the install: rows 5 to 9 and bootstrap are not run when the wait runs out') }
    if ($lines -ccontains (Get-PatchedLine $hostId)) { $p.Add('Patched was printed') }
    Write-HpProblems "${pv}: a provisioning run whose host never finishes its patch run fails at step 6 with the wait's own message, exit 1, and runs nothing after the install (step 6; The patch wait, the deadline)" $p

    $p = New-HpProblems
    $shape = Get-HpLogShape $log
    if ($shape -cne 'run begun,deploy done,start done,sync failed') { $p.Add('The log''s lines are not: run begun,deploy done,start done,sync failed') }
    else {
      $last = $log.Lines[$log.Lines.Count - 1]
      if ($last.Rest -cne (' reason=' + $msg.Substring('agentspace: '.Length))) { $p.Add('The failed line''s reason is not <id> did not finish its patch run within 30 minutes; check: ssh agentspace-host.<project> journalctl -u agentspace-patch, in full') }
    }
    $fin = Get-HpRecord $h 'agentspace'
    if ($null -eq $fin -or $fin['state'] -cne 'failed' -or $fin['instance_id'] -cne $hostId) { $p.Add('The host is not recorded failed with its instance ID') }
    if (-not (Test-HpSameBytes (Get-HpFileBytes (Get-HpLeasesFile $h)) $leasesBefore)) { $p.Add('The lease state file changed: the project''s lease was not left pending') }
    if (-not (Test-HpFreeLock (Get-HpLockFile $h 'agentspace'))) { $p.Add('provision.lock is still held after the run ended') }
    Write-HpProblems "${pv}: the log gets sync failed with the wait's whole message as its reason and no later line, a host that was never ready is recorded failed with its instance ID, the project's lease is left pending and provision.lock is released (step 6; When step 4, 5, 6 or 7 fails)" $p

    $p = New-HpProblems
    if ($r.TimedOut) { $p.Add('It was still running after 35 minutes') }
    elseif ($clock.Elapsed.TotalSeconds -lt $deadlineMinSeconds) { $p.Add('It gave up in less than 29 minutes') }
    elseif ($clock.Elapsed.TotalSeconds -gt $deadlineMaxSeconds) { $p.Add('It gave up after more than 32 minutes') }
    Write-HpProblems "${pv}: the run's wait gives up 30 minutes after it starts, with the last pause cut short to end on it (step 6; P13; the timing is loose, and covers the run's own steps before the wait)" $p
  }
}

Exit-ConformanceRun

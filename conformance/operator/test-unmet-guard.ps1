#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `agentspace-operator guard` (docs/operator.md, "guard", "A host's part that
# fails", "What the guard can race", "Its log", "Its streams, exit codes and help", "Calls by command";
# docs/state.md, "The guard's files", "Live leases and the lease horizon"; D56, D57, D63; the dev lead's
# pins P1 to P4, P10 to P12, P18 and P24). Card #0182 (E0.08-T04-S01).
#
# What it does: it runs the operator in a throwaway home with a stand-in `aws` that logs every call and
# answers from files beside it, and tripwires for `ssh` and `scp` (conformance/operator/host-commands-
# common.ps1 and host-guard-common.ps1), over a host registry and a lease state file the case writes,
# with the home's CA moved away. The guard has no clock seam (operator.md, guard, "Its seams for
# conformance"), so every time is placed relative to the real clock, to the whole second, and no time is
# within 3 minutes of the grace's boundary: every outcome is the same for every moment the run can take.
# Where a time is the run's own (a project whose only live leases are pending has the run's time as its
# horizon), the case accepts every second from the run's start to its end.
#
# Cases: an explained host untouched; a pending lease explaining its host; an unexplained host not yet
# due, logged with its stop time; the decision over eight hosts (the box among them) with each lease
# state and each end, the stops, the sends, the log and the progress lines in the registry's order;
# hosts that are not running; a running host with no usable LaunchTime; an instance missing from the
# answer; a registry record with no instance ID, read from its stack; no host with an ID; the describe
# call failing or answering with a text that is not JSON; a held, a missing and an unreadable
# provision.lock; the horizon's command; a held and an unreadable guard.lock; the host registry missing
# or not valid; the lease state file missing, not valid or holding two records with one id; the state
# folder; a failed stop and a failed send, the next run sending again; the sign-in message at each call in
# turn; guard.log's bytes, its control characters and a log that cannot be written; -Quiet; and what the
# guard never does.
#
# CHOICES where operator.md is silent or can be read two ways (each is a question for the dev lead):
#   1. The closing line's `running <r>` for a running host whose LaunchTime failed its part: step 8 counts
#      the hosts step 6 read as `running`, and step 7 sends "each host step 6 read as `running`, but ... a
#      host whose `LaunchTime` failed its part", so such a host is read as running and the closing line counts
#      it. The case that has such hosts asserts the exact line, and a question stays for the architect only
#      if step 6's "running, with its LaunchTime missing" was meant to leave the host out of `<r>`.
#   2. The closing line after a failure that ends the run (step 1 to 3, a failed describe): operator.md's Exit
#      codes says a failure ends the command at once and prints one message, so each such case asserts that
#      message as the only line of stderr, and no closing line. #0034's sign-in message is asserted as the last
#      line of stderr, with no count after it, and with the progress lines of the stops requested before it.
#   3. The `<error>` of `Cannot try <path>: <error>.` is asserted as non-empty text only, since the spec
#      gives the text of guard.lock's error alone; guard.lock's is asserted as `The guard's lock could
#      not be taken: ` and text.
#   4. The lease state file's and the host registry's `<reason>` is asserted as non-empty text, and equal
#      between stderr and the log, for every file but a missing one (`the file does not exist`).
#   5. A "word" of the send's command is a run of digits not touching a letter, digit, `_`, `.`, `-` or `/`.
#   6. `guard.log` that cannot be written: the order of its message among the progress lines is not
#      asserted. A dropped line is not a host's part that failed, so step 8's `agentspace: <n> of <m> hosts
#      failed.` does not follow, and the case asserts that no such line is printed.
#   7. A `creating` record that holds an instance ID is decided like any host, by its instance's state.
#   8. The decision case's expired lease ends 20 minutes ago, later than the 30 minutes of the lease released
#      after it expired, so the project's last lease end is the expired lease's `expires_at` and a build that
#      gave an expired ready lease no end, or used the later `released_at`, decides that host otherwise.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. AGENTSPACE_AWS_CMD, AGENTSPACE_SSH_CMD and
# AGENTSPACE_SCP_CMD name the stand-ins on every run. Every instance ID is made up, and a failure reason
# never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-guard.ps1
. (Join-Path $PSScriptRoot 'host-guard-common.ps1')

$ErrorActionPreference = 'Stop'
$signInMessage = "agentspace: $script:GdSignIn"
$stopDenied = 'An error occurred (UnauthorizedOperation) when calling the StopInstances operation: You are not authorized to perform this operation.'
$sendDenied = 'An error occurred (InvalidInstanceId) when calling the SendCommand operation: The instance is not in a valid state.'
$describeDenied = 'An error occurred (UnauthorizedOperation) when calling the DescribeInstances operation: You are not authorized to perform this operation.'

Invoke-HostFile -FileName 'test-unmet-guard.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox
  $i1 = Get-GdId 1; $i2 = Get-GdId 2; $i3 = Get-GdId 3; $i4 = Get-GdId 4; $i5 = Get-GdId 5; $i6 = Get-GdId 6; $i7 = Get-GdId 7
  $g = 'operator.md#guard'
  $gPart = 'operator.md#a-hosts-part-that-fails'
  $gRace = 'operator.md#what-the-guard-can-race'
  $gLog = 'operator.md#its-log'
  $gStreams = 'operator.md#its-streams-exit-codes-and-help'
  $gCalls = 'operator.md#calls-by-command'
  $sLock = 'state.md#the-guards-lock'
  $sLog = 'state.md#the-guards-log'
  $sLive = 'state.md#live-leases-and-the-lease-horizon'
  # The box, stopped and so left alone, is in every registry the cases write: it is the registry's first record.
  $boxStopped = New-GdInstance -Id $idBox -State 'stopped'
  $rec = { param([string]$Project, [string]$Id) New-HostRecord -Project $Project -InstanceId $Id }
  $reasonOf = { param([string]$Line) $Line -replace '^agentspace: ', '' }

  # ============ Explained, and pending ============
  $cA = @{ Box = (Get-GdAhead 90 11); Low = (Get-GdAhead 120 22); High = (Get-GdAhead 180 33) }
  Test-GuardCase -Name "${g}: a host a live lease explains is untouched: no stop, no log line, and its horizon is the latest live expires_at (the box by swimcoach's lease, agentspace's host by the later of its two ready leases with a pending one beside them)" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 3 2 0 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-GdCallSend $idBox), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -None
      Get-GdSendProblems -Expected @{ $idBox = $cA.Box; $i1 = $cA.High } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'agentspace' $i1), (& $rec 'gamma' $i2))
    Set-GdLeases @(
      (New-GdLease swimcoach ready -Expires $cA.Box), (New-GdLease agentspace ready -Expires $cA.Low), (New-GdLease agentspace ready -Expires $cA.High),
      (New-GdLease agentspace pending), (New-GdLease gamma released -Released (Get-GdAgo 300) -Expires (Get-GdAgo 200)))
    Set-GdAws -Instance @((New-GdInstance -Id $idBox -Launch (Get-GdAgo 360)), (New-GdInstance -Id $i1 -Launch (Get-GdAgo 360)), (New-GdInstance -Id $i2 -State 'stopped'))
  }
  Test-GuardCase -Name "${sLive}: a project whose only live lease is pending explains its host, and its horizon is the run's time (P10)" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 2 1 0 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1)), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -None
      Get-GdSendProblems -Expected @{ $i1 = 'run' } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'agentspace' $i1))
    Set-GdLeases @((New-GdLease agentspace pending), (New-GdLease agentspace released -Released (Get-GdAgo 300)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch (Get-GdAgo 360)))
  }

  # ============ Unexplained, not yet due ============
  $cC = @{ Launch = (Get-GdAgo 5 37) }
  Test-GuardCase -Name "${g}: an unexplained host inside the grace is logged unexplained with its stop time and not stopped; a project that never had a lease is sent its launch time; another project's lease, one named alike, explains nothing" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressUnexplained 'gamma' $i2 $cC.Launch), (Get-GdClosing 2 1 0 1)))
    Calls = @((Get-GdCallDescribe @($idBox, $i2)), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineUnexplained 'gamma' $cC.Launch))
      Get-GdSendProblems -Expected @{ $i2 = $cC.Launch } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'gamma' $i2))
    Set-GdLeases @((New-GdLease gam ready -Expires (Get-GdAhead 120)), (New-GdLease gamma2 ready -Expires (Get-GdAhead 120)), (New-GdLease agentspace pending))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i2 -Launch $cC.Launch))
  }

  # ============ The decision over eight hosts ============
  # Registry order: box, zeta, delta, agentspace, gamma, theta, epsilon, eta. Every instance's id is in the filter in that order.
  $cD = [ordered]@{
    Box = (Get-GdAgo 180 11); Zeta = (Get-GdAgo 8 21); Delta = (Get-GdAgo 25 43); Gamma = (Get-GdAgo 120 17)
    EpsExpired = (Get-GdAgo 20 5); EpsReleased = (Get-GdAgo 5 9); EpsEnd = (Get-GdAgo 30 52)
    Eta = (Get-GdAgo 3 29); EtaOld = (Get-GdAgo 120 30); Ag = (Get-GdAhead 60 14)
  }
  $dIds = @($idBox, $i5, $i3, $i1, $i2, $i7, $i4, $i6)
  $dSetup = {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'zeta' $i5), (& $rec 'delta' $i3), (& $rec 'agentspace' $i1), (& $rec 'gamma' $i2), (& $rec 'theta' $i7), (& $rec 'epsilon' $i4), (& $rec 'eta' $i6))
    Set-GdLeases @(
      (New-GdLease agentspace ready -Expires $cD.Ag),
      (New-GdLease zeta released -Released $cD.Zeta -Expires (Get-GdAhead 60)),
      (New-GdLease delta released -Released $cD.Delta),
      (New-GdLease gamma unsatisfiable),
      (New-GdLease epsilon ready -Expires $cD.EpsExpired), (New-GdLease epsilon released -Released $cD.EpsReleased -Expires $cD.EpsEnd),
      (New-GdLease eta ready -Expires $cD.EtaOld))
    $etaText = $cD.Eta.AddHours(-7).ToString("yyyy-MM-dd'T'HH:mm:ss'.912-07:00'", [Globalization.CultureInfo]::InvariantCulture)
    Set-GdAws -Instance @(
      (New-GdInstance -Id $idBox -Launch $cD.Box), (New-GdInstance -Id $i5 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i3 -Launch (Get-GdAgo 240)),
      (New-GdInstance -Id $i1 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i2 -Launch $cD.Gamma), (New-GdInstance -Id $i7 -State 'stopped'),
      (New-GdInstance -Id $i4 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i6 -LaunchText $etaText))
  }
  $dCalls = @(
    (Get-GdCallDescribe $dIds),
    (Get-CallStop $idBox), (Get-CallStop $i3), (Get-CallStop $i2), (Get-CallStop $i4),
    (Get-GdCallSend $i5), (Get-GdCallSend $i1), (Get-GdCallSend $i6))
  $dLog = @(
    (Get-GdLineStopped 'swimcoach' $cD.Box), (Get-GdLineUnexplained 'zeta' $cD.Zeta), (Get-GdLineStopped 'delta' $cD.Delta),
    (Get-GdLineStopped 'gamma' $cD.Gamma), (Get-GdLineStopped 'epsilon' $cD.EpsExpired), (Get-GdLineUnexplained 'eta' $cD.Eta))
  $dCheck = { param($run, $started)
    Get-GdLogProblems -Run $run -Started $started -Body $dLog
    Get-GdSendProblems -Expected @{ $i5 = $cD.Zeta; $i1 = $cD.Ag; $i6 = $cD.Eta } -Run $run -Started $started }
  Test-GuardCase -Name "${g}: the decision over eight hosts in the registry's order, one describe call naming every ID: explained (a live ready lease) untouched; a project with no live lease counted from the later of its launch and its last lease end (a released lease's released_at, a released never-ready lease's released_at, an unsatisfiable lease adding none, the earlier of released_at and expires_at for a lease released after it expired, an expired lease's expires_at, the launch when it is the later); due hosts stopped in order, the box among them; inside the grace logged unexplained; a stopped host untouched; the horizon sent to each running host the run did not stop (the launch time, when it is later than the last lease end, from a LaunchTime with a fraction and an offset)" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @(
        (Get-GdProgressStopping 'swimcoach' $idBox), (Get-GdProgressUnexplained 'zeta' $i5 $cD.Zeta), (Get-GdProgressStopping 'delta' $i3),
        (Get-GdProgressStopping 'gamma' $i2), (Get-GdProgressStopping 'epsilon' $i4), (Get-GdProgressUnexplained 'eta' $i6 $cD.Eta),
        (Get-GdClosing 8 7 4 2)))
    Calls = $dCalls; Check = $dCheck } -Setup $dSetup
  Test-GuardCase -Name "${gStreams}: guard -Quiet prints neither the progress lines nor the closing line, and does what it does without it: the same calls, the same log, exit 0, stderr empty" -Arguments @('guard', '-Quiet') -Expect @{
    Exit = 0; Stderr = ''; Calls = $dCalls; Check = $dCheck } -Setup $dSetup

  # ============ A host that is not running ============
  Test-GuardCase -Name "${g}: a host that is pending, stopping, stopped, shutting-down or terminated gets no line, no stop and no send, whatever its project's leases, and only the describe call is made" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 6 0 0 0)))
    Calls = @(, (Get-GdCallDescribe @($idBox, $i1, $i2, $i3, $i4, $i5)))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -None } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'pend' $i1), (& $rec 'stopg' $i2), (& $rec 'shut' $i3), (& $rec 'term' $i4), (& $rec 'idle' $i5))
    Set-GdLeases @((New-GdLease idle released -Released (Get-GdAgo 300)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -State 'pending'), (New-GdInstance -Id $i2 -State 'stopping'), (New-GdInstance -Id $i3 -State 'shutting-down'), (New-GdInstance -Id $i4 -State 'terminated'), (New-GdInstance -Id $i5 -State 'stopped'))
  }

  # ============ A running host with no usable LaunchTime ============
  $cG = @{ Launch = (Get-GdAgo 240); Exp = (Get-GdAhead 60) }
  $launchMissing = 'Unexpected answer from ec2 describe-instances: no Reservations[].Instances[].LaunchTime.'
  Test-GuardCase -Name "${g}: a running host with no LaunchTime, or one not of its form, explained by a live lease or not, fails its part alone at its place in the registry's order: no stop, no send, its failed step=instance line; the other hosts are guarded; 3 of 6 hosts failed, exit 1" -Expect @{
    Exit = 1
    Check = { param($run, $started)
      Get-GdStderrProblems -Stderr $run.Stderr -Want @(
        (Get-GdHostFailure 'alpha' $launchMissing), (Get-GdHostFailure 'bravo' $launchMissing), (Get-GdHostFailure 'charlie' $launchMissing),
        (Get-GdProgressStopping 'delta' $i4), (Get-GdClosing 6 5 1 0), (Get-GdFailedCount 3 6))
      Get-GdLogProblems -Run $run -Started $started -Body @(
        (Get-GdLineFailed 'alpha' 'instance' $launchMissing), (Get-GdLineFailed 'bravo' 'instance' $launchMissing),
        (Get-GdLineFailed 'charlie' 'instance' $launchMissing), (Get-GdLineStopped 'delta' $cG.Launch))
      Get-GdSendProblems -Expected @{ $i5 = $cG.Exp } -Run $run -Started $started }
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3, $i4, $i5)), (Get-CallStop $i4), (Get-GdCallSend $i5)) } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3), (& $rec 'delta' $i4), (& $rec 'echo' $i5))
    Set-GdLeases @((New-GdLease alpha ready -Expires (Get-GdAhead 120)), (New-GdLease echo ready -Expires $cG.Exp))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -NoLaunch), (New-GdInstance -Id $i2 -NoLaunch), (New-GdInstance -Id $i3 -LaunchText '2026-11-22T10:00:00'), (New-GdInstance -Id $i4 -Launch $cG.Launch), (New-GdInstance -Id $i5 -Launch (Get-GdAgo 240)))
  }

  # ============ An instance missing from the answer ============
  $cH = @{ Launch = (Get-GdAgo 240) }
  Test-GuardCase -Name "${g}: a host whose instance is not in the answer fails its part with Instance <id> was not found., at its place in the registry's order, and the others are guarded; elements of the answer that are no host's, or have no InstanceId, are ignored" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'alpha' "Instance $i1 was not found."), (Get-GdProgressStopping 'bravo' $i2), (Get-GdClosing 3 1 1 0), (Get-GdFailedCount 1 3)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-CallStop $i2))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'alpha' 'instance' "Instance $i1 was not found."), (Get-GdLineStopped 'bravo' $cH.Launch)) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2))
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i2 -Launch $cH.Launch))
  }

  # ============ A record with no instance ID ============
  $cI = @{ Launch = (Get-GdAgo 240); Ag = (Get-GdAhead 90) }
  $noOutput = "Stack 'agentspace-host-delta' has no InstanceId output."
  $noStack = 'An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id agentspace-host-echo does not exist'
  Test-GuardCase -Name "${g}: a ready record with no instance ID has it read from its stack, in the registry's order, before the one describe call, and not written back; a creating or failed record with none is skipped with no call and no line; a stack with no InstanceId output, and one that fails, fail that host's part alone while the others are guarded; a creating record that holds an ID is decided like any host" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'delta' $noOutput), (Get-GdHostFailure 'echo' $noStack), (Get-GdProgressStopping 'foxtrot' $i6), (Get-GdClosing 7 2 1 0), (Get-GdFailedCount 2 7)))
    Calls = @(
      (Get-CallStack 'agentspace-host-alpha'), (Get-CallStack 'agentspace-host-delta'), (Get-CallStack 'agentspace-host-echo'),
      (Get-GdCallDescribe @($idBox, $i1, $i6)), (Get-CallStop $i6), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'delta' 'instance' $noOutput), (Get-GdLineFailed 'echo' 'instance' $noStack), (Get-GdLineStopped 'foxtrot' $cI.Launch))
      Get-GdSendProblems -Expected @{ $i1 = $cI.Ag } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-HostRecord -Project 'alpha'), (New-HostRecord -Project 'bravo' -State 'creating'), (New-HostRecord -Project 'charlie' -State 'failed'),
      (New-HostRecord -Project 'delta'), (New-HostRecord -Project 'echo'), (New-HostRecord -Project 'foxtrot' -State 'creating' -InstanceId $i6))
    Set-GdLeases @((New-GdLease alpha ready -Expires $cI.Ag))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cI.Launch), (New-GdInstance -Id $i6 -Launch $cI.Launch)) -More @{
      'cloudformation describe-stacks agentspace-host-alpha' = @(New-Step (New-StackDoc -StackName 'agentspace-host-alpha' -Id $i1))
      'cloudformation describe-stacks agentspace-host-delta' = @(New-Step (New-StackDoc -StackName 'agentspace-host-delta'))
      'cloudformation describe-stacks agentspace-host-echo' = @(New-Step '' $noStack 254)
    }
  }
  Test-GuardCase -Name "${g}: with no host holding an ID, and the one stack read failing, no describe call is made and the host's part fails; a creating record with none is skipped" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'swimcoach' $noStack), (Get-GdClosing 2 0 0 0), (Get-GdFailedCount 1 2)))
    Calls = @(, (Get-CallStack 'agentbox'))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'swimcoach' 'instance' $noStack)) } } -Setup {
    Set-HostsRecords @((New-HostRecord -Project 'swimcoach'), (New-HostRecord -Project 'bravo' -State 'creating'))
    Set-GdLeases @()
    Set-GdAws -More @{ 'cloudformation describe-stacks agentbox' = @(New-Step '' $noStack 254) }
  }

  # ============ The describe call fails ============
  $cK = @{ Launch = (Get-GdAgo 240) }
  Test-GuardCase -Name "${g}: a failed ec2 describe-instances fails the run: its message on stderr, one failed step=describe line with no host= field, no stop and no send for a host that is due, exit 1" -Expect @{
    Exit = 1
    Calls = @(, (Get-GdCallDescribe @($idBox, $i1)))
    Check = { param($run, $started)
      Get-GdStderrProblems -Stderr $run.Stderr -Want @("agentspace: $describeDenied")
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineRunFailed 'describe' $describeDenied)) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1))
    Set-GdLeases @()
    Set-GdAws -More @{ 'ec2 describe-instances' = @(New-Step '' $describeDenied 254) }
  }
  Test-GuardCase -Name "${g}: an ec2 describe-instances answer that is not JSON fails the run with Unexpected answer from ec2 describe-instances: not JSON., logged failed step=describe, and nothing is stopped or sent" -Expect @{
    Exit = 1
    Calls = @(, (Get-GdCallDescribe @($idBox, $i1)))
    Check = { param($run, $started)
      Get-GdStderrProblems -Stderr $run.Stderr -Want @('agentspace: Unexpected answer from ec2 describe-instances: not JSON.')
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineRunFailed 'describe' 'Unexpected answer from ec2 describe-instances: not JSON.')) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1))
    Set-GdLeases @()
    Set-GdAws -More @{ 'ec2 describe-instances' = @(New-Step 'this is not JSON' '' 0) }
  }

  # ============ The provisioning lock ============
  $cL = @{ Launch = (Get-GdAgo 240) }
  $lockDelta = Get-GdProvisionLockPath 'delta'
  Test-GuardCase -Name "${g}: a due host whose provision.lock is held is not stopped and is logged unexplained with provisioning=running, and still sent its horizon; a missing hosts\<project>\ folder and a missing lock file are free, and neither is made; a folder at the lock's path fails the host's part with Cannot try <path>: <error>. and the host is not stopped" -Expect @{
    Exit = 1
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3, $i4)), (Get-CallStop $i2), (Get-CallStop $i3), (Get-GdCallSend $i1), (Get-GdCallSend $i4))
    Check = { param($run, $started)
      Get-GdStderrProblems -Stderr $run.Stderr -Want @(
        (Get-GdProgressProvisioning 'alpha' $i1), (Get-GdProgressStopping 'bravo' $i2), (Get-GdProgressStopping 'charlie' $i3),
        ('re:^agentspace: Host ''delta'': Cannot try ' + [regex]::Escape($lockDelta) + ': .+\.$'),
        (Get-GdClosing 5 4 2 1), (Get-GdFailedCount 1 5))
      Get-GdLogProblems -Run $run -Started $started -Body @(
        (Get-GdLineUnexplained 'alpha' $cL.Launch -Provisioning), (Get-GdLineStopped 'bravo' $cL.Launch), (Get-GdLineStopped 'charlie' $cL.Launch),
        ('re:^host=delta failed step=lock reason=Cannot try ' + [regex]::Escape($lockDelta) + ': .+\.$'))
      Get-GdSendProblems -Expected @{ $i1 = $cL.Launch; $i4 = $cL.Launch } -Run $run -Started $started
      if (Test-Path -LiteralPath (Join-Path $fx.HostsDir 'bravo')) { 'The guard made the folder of a host with no provisioning run' }
      if (Test-Path -LiteralPath (Get-GdProvisionLockPath 'charlie')) { 'The guard made a lock file to try it' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3), (& $rec 'delta' $i4))
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cL.Launch), (New-GdInstance -Id $i2 -Launch $cL.Launch), (New-GdInstance -Id $i3 -Launch $cL.Launch), (New-GdInstance -Id $i4 -Launch $cL.Launch))
    Hold-GdLock (Get-GdProvisionLockPath 'alpha')
    New-HomeFolder (Join-Path $fx.HostsDir 'charlie')
    New-GdFolderAt (Get-GdProvisionLockPath 'delta')
  }

  # ============ guard.lock ============
  $cM = @{ Launch = (Get-GdAgo 240) }
  Test-GuardCase -Name "${sLock}: a guard.lock another program holds ends the run at once with exit 0: nothing on stdout or stderr, no AWS call, no line in guard.log, and a host that is due is not stopped" -Expect @{
    Exit = 0; Stderr = ''; Calls = @()
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -None } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1))
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cM.Launch))
    Hold-GdLock (Get-GdLockPath)
  }
  Test-GuardCase -Name "${sLock}: a held guard.lock is met before anything is read: with a host registry that is not valid and no lease state file, the run still exits 0 with nothing printed, called or logged" -Expect @{
    Exit = 0; Stderr = ''; Calls = @()
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -None } } -Setup {
    Set-HostsFile ([Text.Encoding]::UTF8.GetBytes('{"version":0,"hosts":['))
    Hold-GdLock (Get-GdLockPath)
  }
  $lockPath = Get-GdLockPath
  Test-GuardCase -Name "${g}: a guard.lock that cannot be taken for another reason, a folder at its path, fails the run with agentspace: Cannot take <path>: <error>., logged failed step=lock with no host= field, no AWS call, exit 1" -Expect @{
    Exit = 1; Calls = @()
    Check = { param($run, $started)
      Get-GdStderrProblems -Stderr $run.Stderr -Want @(('re:^agentspace: Cannot take ' + [regex]::Escape($lockPath) + ': The guard''s lock could not be taken: .+\.$'))
      Get-GdLogProblems -Run $run -Started $started -Body @(('re:^failed step=lock reason=Cannot take ' + [regex]::Escape($lockPath) + ': The guard''s lock could not be taken: .+\.$')) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1))
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cM.Launch))
    New-GdFolderAt (Get-GdLockPath)
  }
  Test-GuardCase -Name "${sLock}: the guard holds guard.lock as a run and leaves it: created in %USERPROFILE%\.agentspace and not in the state folder, empty, free when the run has ended, and a run with nothing to log creates no guard.log" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 1 0 0 0)))
    Calls = @(, (Get-GdCallDescribe @($idBox)))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -None
      $lock = Get-GdLockPath
      if (-not (Test-Path -LiteralPath $lock -PathType Leaf)) { 'guard.lock was not made' }
      else {
        if ((Get-Item -LiteralPath $lock).Length -ne 0) { 'guard.lock is not empty' }
        if (-not (Test-GdFree $lock)) { 'guard.lock is still held after the run' }
      }
      if (Test-Path -LiteralPath (Join-Path $fx.H.StateDir 'guard.lock')) { 'guard.lock was made in the state folder' } } } -Setup {
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped)
  }

  # ============ The host registry ============
  $hostsPath = $fx.HostsFile
  Test-GuardCase -Name "${g}: a host registry that is missing is the box's record alone: its instance ID is read from the agentbox stack and not written back, and a due box is stopped like any host" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping 'swimcoach' $idBox), (Get-GdClosing 1 1 1 0)))
    Calls = @((Get-CallStack 'agentbox'), (Get-GdCallDescribe @($idBox)), (Get-CallStop $idBox))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped 'swimcoach' $cM.Launch))
      if (Test-Path -LiteralPath $hostsPath) { 'hosts.json was written' } } } -Setup {
    Remove-HomePath $fx.HostsFile
    Set-GdLeases @()
    Set-GdAws -Instance @((New-GdInstance -Id $idBox -Launch $cM.Launch)) -More @{ 'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $idBox)) }
  }
  $registryBad = [ordered]@{
    'not JSON' = { Set-HostsFile ([Text.Encoding]::UTF8.GetBytes('{"version":0,"hosts":[')) }
    'no record for the box' = { Set-HostsRecords @((New-HostRecord -Project 'agentspace')) }
    'two records for one project' = { Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'alpha' $i2)) }
  }
  foreach ($variant in $registryBad.Keys) {
    $cP = @{ Make = $registryBad[$variant] }
    Test-GuardCase -Name "${g}: a host registry that is not valid ($variant) fails the run with The host registry <path> cannot be read: <reason>., logged failed step=hosts with no host= field, no AWS call, nothing stopped, exit 1, and the file is not repaired" -Expect @{
      Exit = 1; Calls = @()
      Check = { param($run, $started)
        $pattern = 'The host registry ' + [regex]::Escape($hostsPath) + ' cannot be read: .+\.'
        Get-GdStderrProblems -Stderr $run.Stderr -Want @("re:^agentspace: $pattern`$")
        Get-GdLogProblems -Run $run -Started $started -Body @("re:^failed step=hosts reason=$pattern`$")
        $first = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })[0]
        if ($null -ne $first -and @(Get-GdLogTexts).Count -gt 0 -and (@(Get-GdLogTexts)[0] -cne ('failed step=hosts reason=' + (& $reasonOf $first)))) { 'The log''s reason is not the message on stderr' } } } -Setup {
      Set-GdLeases @()
      & $cP.Make
    }
  }

  # ============ The lease state file ============
  $leasesPath = Get-GdLeasesPath
  $leaseBad = [ordered]@{
    'missing' = { }
    'not JSON' = { Set-GdLeasesText '{"version":0,"leases":[' }
    'version 1' = { Set-GdLeasesText ('{"version":1,"leases":[]}' + "`n") }
    'two records with one id' = { Set-GdLeases @((New-GdLease alpha released -Released (Get-GdAgo 300) -Id '00000000-0000-4000-8000-000000000001'), (New-GdLease alpha released -Released (Get-GdAgo 200) -Id '00000000-0000-4000-8000-000000000001')) }
    'a time that names no real instant' = { Set-GdLeasesText ('{"version":0,"leases":[{"id":"00000000-0000-4000-8000-000000000002","project":"alpha","state":"released","platform":"linux/arm64","profile":"agent-linux@3","size":"m","ttl_seconds":3600,"created_at":"2026-01-01T00:00:00Z","released_at":"2026-02-30T00:00:00Z","expires_at":"2026-02-30T01:00:00Z","driver":"ec2"}]}' + "`n") }
    'a folder at its path' = { New-GdFolderAt (Get-GdLeasesPath) }
  }
  foreach ($variant in $leaseBad.Keys) {
    $cR = @{ Make = $leaseBad[$variant]; Missing = ($variant -eq 'missing') }
    Test-GuardCase -Name "${g}: a lease state file that is $variant fails the run (P18): The lease state file <path> cannot be read: <reason>. on stderr, exactly one failed step=leases line with no host= field, no AWS call at all (no stack read, no stop, no send) though a host is running past its stop time, exit 1" -Expect @{
      Exit = 1; Calls = @()
      Check = { param($run, $started)
        $pattern = if ($cR.Missing) { 'The lease state file ' + [regex]::Escape($leasesPath) + ' cannot be read: the file does not exist\.' } else { 'The lease state file ' + [regex]::Escape($leasesPath) + ' cannot be read: .+\.' }
        Get-GdStderrProblems -Stderr $run.Stderr -Want @("re:^agentspace: $pattern`$")
        Get-GdLogProblems -Run $run -Started $started -Body @("re:^failed step=leases reason=$pattern`$")
        $first = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })[0]
        if ($null -ne $first -and @(Get-GdLogTexts).Count -gt 0 -and (@(Get-GdLogTexts)[0] -cne ('failed step=leases reason=' + (& $reasonOf $first)))) { 'The log''s reason is not the message on stderr' } } } -Setup {
      Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (New-HostRecord -Project 'bravo'))
      Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch (Get-GdAgo 300)))
      & $cR.Make
    }
  }

  # ============ The state folder ============
  $otherState = Join-Path $fx.H.UserProfile 'other-state'
  $cY = @{ Launch = (Get-GdAgo 300); Exp = (Get-GdAhead 120) }
  $ySetup = {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1))
    Set-GdLeases @((New-GdLease alpha ready -Expires $cY.Exp)) -Dir $otherState
    Set-GdLeases @((New-GdLease alpha released -Released (Get-GdAgo 200)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cY.Launch))
  }
  # The first case adds a host no lease explains, due to be stopped, so that a line is logged and the log's place is shown.
  $ySetupLog = {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'beta' $i2))
    Set-GdLeases @((New-GdLease alpha ready -Expires $cY.Exp)) -Dir $otherState
    Set-GdLeases @((New-GdLease alpha released -Released (Get-GdAgo 200)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cY.Launch), (New-GdInstance -Id $i2 -Launch $cY.Launch))
  }
  Test-GuardCase -Name "${gRace}: AGENTSPACE_STATE_DIR moves the lease state file the guard reads, and nothing else: the host that the other folder's lease explains is not stopped, and guard.log, guard.lock and hosts.json stay under the home's .agentspace" -Environment @{ AGENTSPACE_STATE_DIR = $otherState } -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping 'beta' $i2), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-CallStop $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped 'beta' $cY.Launch))
      Get-GdSendProblems -Expected @{ $i1 = $cY.Exp } -Run $run -Started $started
      $extra = @(Get-ChildItem -LiteralPath $otherState -Force | Where-Object { $_.Name -cne 'leases.json' })
      if ($extra.Count -gt 0) { "The state folder holds $($extra.Count) file or folder beside leases.json" }
      if (-not (Test-Path -LiteralPath (Get-GdLockPath))) { 'guard.lock was not made beside hosts.json' } } } -Setup $ySetupLog
  Test-GuardCase -Name "${gRace}: without AGENTSPACE_STATE_DIR naming it, the lease in another state folder explains no host: the same host is stopped, its stop time counted from the home's own lease state file" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping 'alpha' $i1), (Get-GdClosing 2 1 1 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1)), (Get-CallStop $i1))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -Body @(('re:^host=alpha stopped since=\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$')) } } -Setup $ySetup

  # ============ A failed stop ============
  $cS = @{ Launch = (Get-GdAgo 240); Exp = (Get-GdAhead 60) }
  $sSetup = {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3))
    Set-GdLeases @((New-GdLease charlie ready -Expires $cS.Exp))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cS.Launch), (New-GdInstance -Id $i2 -Launch $cS.Launch), (New-GdInstance -Id $i3 -Launch $cS.Launch)) -More @{
      "ec2 stop-instances $i1" = @(New-Step '' $stopDenied 254)
    }
  }
  $sCalls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3)), (Get-CallStop $i1), (Get-CallStop $i2), (Get-GdCallSend $i1), (Get-GdCallSend $i3))
  $sCheck = { param($run, $started)
    Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'alpha' 'stop' "Could not stop ${i1}: $stopDenied"), (Get-GdLineStopped 'bravo' $cS.Launch))
    Get-GdSendProblems -Expected @{ $i1 = $cS.Launch; $i3 = $cS.Exp } -Run $run -Started $started }
  Test-GuardCase -Name "${gPart}: a stop that fails fails that host's part with Could not stop <id>: <error> and logs failed step=stop; the run goes on, the next due host is stopped, the host that was not stopped is sent its horizon, and agentspace: 1 of 4 hosts failed. is the last line, exit 1" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'alpha' "Could not stop ${i1}: $stopDenied"), (Get-GdProgressStopping 'bravo' $i2), (Get-GdClosing 4 3 1 0), (Get-GdFailedCount 1 4)))
    Calls = $sCalls; Check = $sCheck } -Setup $sSetup
  Test-GuardCase -Name "${gStreams}: with -Quiet a failure's message and the count line still print, and the progress lines and the closing line do not; the log and the calls are the same" -Arguments @('guard', '-Quiet') -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'alpha' "Could not stop ${i1}: $stopDenied"), (Get-GdFailedCount 1 4)))
    Calls = $sCalls; Check = $sCheck } -Setup $sSetup

  # ============ A failed send, and the next run sending again ============
  $cT = @{ Ag = (Get-GdAhead 90); Br = (Get-GdAhead 150) }
  Test-GuardCase -Name "${g}: a horizon send that fails fails that host's part with Could not send the lease horizon to <id>: <error> and logs failed step=horizon; every other host is still sent its horizon, exit 1; the next run sends to every running host again, whether or not the last send reached it, and a horizon that was sent logs nothing" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure 'alpha' "Could not send the lease horizon to ${i1}: $sendDenied"), (Get-GdClosing 3 2 0 0), (Get-GdFailedCount 1 3)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-GdCallSend $i1), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'alpha' 'horizon' "Could not send the lease horizon to ${i1}: $sendDenied"))
      Get-GdSendProblems -Expected @{ $i1 = $cT.Ag; $i2 = $cT.Br } -Run $run -Started $started
      # The second run: the same state, the send now accepted.
      Clear-StandinLogs
      Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i2 -Launch (Get-GdAgo 240)))
      $again = Invoke-HostRun -Arguments @('guard')
      $againStarted = $again.Started
      if ($again.ExitCode -ne 0) { "The next run exited $($again.ExitCode), not 0" }
      $sent = @(Get-GdSendCalls)
      if ($sent.Count -ne 2) { "The next run made $($sent.Count) sends, not 2" }
      Get-GdSendProblems -Expected @{ $i1 = $cT.Ag; $i2 = $cT.Br } -Run $again -Started $againStarted
      if (@(Get-GdLogTexts).Count -ne 1) { 'The next run logged a line for a horizon that was sent' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2))
    Set-GdLeases @((New-GdLease alpha ready -Expires $cT.Ag), (New-GdLease bravo ready -Expires $cT.Br))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i2 -Launch (Get-GdAgo 240))) -More @{
      "ssm send-command $i1" = @(New-Step '' $sendDenied 254)
    }
  }

  # ============ #0034's sign-in message at each call in turn ============
  $cU = @{ Launch = (Get-GdAgo 240); Exp = (Get-GdAhead 60) }
  $signInCases = @(
    @{ Step = 'instance'; Label = 'the stack read'; Calls = @(, (Get-CallStack 'agentspace-host-alpha')); Key = 'cloudformation describe-stacks agentspace-host-alpha'; Setup = { Set-HostsRecords @((New-BoxRecord $idBox), (New-HostRecord -Project 'alpha')); Set-GdLeases @() }; Progress = @(); BodyBefore = @() },
    @{ Step = 'describe'; Label = 'the describe call'; Calls = @(, (Get-GdCallDescribe @($idBox, $i1))); Key = 'ec2 describe-instances'; Setup = { Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1)); Set-GdLeases @() }; Progress = @(); BodyBefore = @() },
    @{ Step = 'stop'; Label = 'a stop, after another host was stopped'; Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3)), (Get-CallStop $i1), (Get-CallStop $i2)); Key = "ec2 stop-instances $i2"
      Setup = { Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3)); Set-GdLeases @() }
      Progress = @((Get-GdProgressStopping 'alpha' $i1)); BodyBefore = @((Get-GdLineStopped 'alpha' $cU.Launch)) },
    @{ Step = 'horizon'; Label = 'a send, after another host was stopped'; Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3)), (Get-CallStop $i1), (Get-GdCallSend $i2)); Key = "ssm send-command $i2"
      Setup = { Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3)); Set-GdLeases @((New-GdLease bravo ready -Expires $cU.Exp), (New-GdLease charlie ready -Expires $cU.Exp)) }
      Progress = @((Get-GdProgressStopping 'alpha' $i1)); BodyBefore = @((Get-GdLineStopped 'alpha' $cU.Launch)) }
  )
  foreach ($sc in $signInCases) {
    $cV = $sc
    Test-GuardCase -Name "${gStreams}: #0034's sign-in message from the stand-in at $($cV.Label): its message is the last line of stderr, one failed step=$($cV.Step) line with no host= field is logged, no later call is made, a stop already requested stands, no failure count follows, exit 1" -Expect @{
      Exit = 1; Calls = $cV.Calls
      Check = { param($run, $started)
        $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
        if ($lines.Count -eq 0 -or $lines[-1] -cne $signInMessage) { 'The sign-in message is not the last line of stderr' }
        if (@($lines | Where-Object { $_ -match ' hosts failed\.$' }).Count -gt 0) { 'A failure count was printed after the sign-in message' }
        foreach ($want in $cV.Progress) { if ($lines -cnotcontains $want) { 'The progress line of the stop that was requested is missing' } }
        Get-GdLogProblems -Run $run -Started $started -Body (@($cV.BodyBefore) + @((Get-GdLineRunFailed $cV.Step $script:GdSignIn))) } } -Setup {
      & $cV.Setup
      $answers = @{}
      $answers[$cV.Key] = @(New-Step '' $script:GdSignInStandIn 255)
      Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cU.Launch), (New-GdInstance -Id $i2 -Launch $cU.Launch), (New-GdInstance -Id $i3 -Launch $cU.Launch)) -More $answers
    }
  }

  # ============ guard.log's bytes ============
  $cW = @{ Launch = (Get-GdAgo 240); Recent = (Get-GdAgo 4 41) }
  $reasonRaw = "Denied`tby policy`nsecond line$([char]1)end"
  $reasonLogged = 'Denied?by policy?second line?end'
  Test-GuardCase -Name "${sLog}: every control character of a failure's reason is written as ? in guard.log, the reason runs to the end of the line, the lines are UTF-8 with no byte order mark, each ends in one LF and holds no carriage return, host= comes right after the time on a host's line alone, and every line is in the form of operator.md" -Expect @{
    Exit = 1
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-CallStop $i1), (Get-GdCallSend $i1), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed 'alpha' 'stop' "Could not stop ${i1}: $reasonLogged"), (Get-GdLineUnexplained 'bravo' $cW.Recent)) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2))
    Set-GdLeases @()
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cW.Launch), (New-GdInstance -Id $i2 -Launch $cW.Recent)) -More @{ "ec2 stop-instances $i1" = @(New-Step '' $reasonRaw 254) }
  }
  $logPath = Get-GdLogPath
  Test-GuardCase -Name "${gLog}: a guard.log that cannot be written (a folder at its path) changes nothing the run decides or does, the stops and the sends are made as without it, agentspace: Cannot write the guard's log <path>: <error>. is printed once, exit 1" -Expect @{
    Exit = 1
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2, $i3)), (Get-CallStop $i1), (Get-GdCallSend $i2), (Get-GdCallSend $i3))
    Check = { param($run, $started)
      $lines = @($run.Stderr -split "`n" | Where-Object { $_ -ne '' })
      $written = @($lines | Where-Object { $_ -cmatch ('^agentspace: Cannot write the guard''s log ' + [regex]::Escape($logPath) + ': .+\.$') })
      if ($written.Count -ne 1) { "The message about the log was printed $($written.Count) times, not once" }
      # A dropped line is no host's part that failed (step 8; Exit codes lists a dropped line apart from a host's part).
      if (@($lines | Where-Object { $_ -cmatch '^agentspace: \d+ of \d+ hosts failed\.$' }).Count -gt 0) { 'A count of hosts that failed was printed, though no host''s part failed' }
      foreach ($want in @((Get-GdProgressStopping 'alpha' $i1), (Get-GdProgressUnexplained 'bravo' $i2 $cW.Recent), (Get-GdClosing 4 3 1 1))) { if ($lines -cnotcontains $want) { 'A progress line, or the closing line, is missing' } }
      if (-not (Test-Path -LiteralPath $logPath -PathType Container)) { 'The folder at the log''s path is gone' }
      elseif (@(Get-ChildItem -LiteralPath $logPath -Force).Count -gt 0) { 'Something was written into the folder at the log''s path' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2), (& $rec 'charlie' $i3))
    Set-GdLeases @((New-GdLease charlie ready -Expires (Get-GdAhead 60)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $cW.Launch), (New-GdInstance -Id $i2 -Launch $cW.Recent), (New-GdInstance -Id $i3 -Launch $cW.Launch))
    New-GdFolderAt (Get-GdLogPath)
  }

  # ============ What the guard never does ============
  Test-GuardCase -Name "${g}: the guard starts no ssh and no scp, makes no operator folder, writes no key, certificate or alias with no CA in the home, takes neither hosts.lock nor leases.lock, and leaves hosts.json and leases.json as they were, over a run that stops a host and sends a horizon" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping 'alpha' $i1), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i1, $i2)), (Get-CallStop $i1), (Get-GdCallSend $i2))
    SshCalls = @(); ScpCalls = @()
    Check = { param($run, $started)
      if (Test-Path -LiteralPath $fx.CaDir) { 'The home still has its CA, so the run was not made with none' } } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (& $rec 'alpha' $i1), (& $rec 'bravo' $i2))
    Set-GdLeases @((New-GdLease bravo ready -Expires (Get-GdAhead 60)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch (Get-GdAgo 240)), (New-GdInstance -Id $i2 -Launch (Get-GdAgo 240)))
  }
}
Exit-ConformanceRun

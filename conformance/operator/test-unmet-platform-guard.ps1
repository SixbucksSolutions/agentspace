#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `guard` over hosts of one project on more than one platform
# (docs/operator.md, "guard", "A host's platform and its names", "The idle check and the lease
# horizon", "Its log"; docs/state.md, "Live leases and the lease horizon"; docs/conformance.md, "v0.12's
# planned cases for the shared and Linux parts", cases W13 to W15; D56, D57, D91; E0.12-T04-S02, card
# #0262, planned by #0248). Every expectation is the spec's text: which host a live lease explains, the
# horizon each host is sent, the progress lines and the log's lines, which name a host off linux/arm64 by
# its host name, and the folder of each host's provision.lock.
#
# What it does: as test-unmet-guard.ps1 does (conformance/operator/host-guard-common.ps1): it runs `guard`
# itself with the seam set, never a detached start, against a stand-in `aws` that answers
# `ec2 describe-instances` from the case's own list, a host registry and a lease state file the case writes
# with times set relative to the real clock, and ssh and scp tripwires. It never touches a real host, and
# no case starts a Windows host: a windows/amd64 record and its running instance are seeded and stopped by
# a stand-in `ec2 stop-instances`.
#
#   W13  A host is explained by a live lease of its own platform alone, in each combination of a live lease
#        on neither, either or both of a project's linux/arm64 and linux/amd64 hosts; a lease that is
#        released, expired or unsatisfiable explains nothing; a running windows/amd64 host whose project
#        holds only Linux leases is unexplained and stopped after the grace.
#   W14  Each host is sent its own horizon with AWS-RunShellScript, the pass visits the hosts and names
#        their IDs in the file's order, and the log and the progress lines name a host off linux/arm64 by
#        its host name.
#   W15  A host's provision.lock is its own: the lock of one platform's folder leaves the other platform's
#        host to be stopped, and a folder that does not exist reads as free and is not made.
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
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-guard.ps1
. (Join-Path $PSScriptRoot 'host-guard-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$ErrorActionPreference = 'Stop'

Invoke-HostFile -FileName 'test-unmet-platform-guard.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox
  $i1 = Get-GdId 1; $i2 = Get-GdId 2; $i3 = Get-GdId 3
  $g = 'operator.md#guard'
  $sLive = 'state.md#live-leases-and-the-lease-horizon'
  $boxStopped = New-GdInstance -Id $idBox -State 'stopped'
  $armName = 'agentspace'
  $ampName = 'agentspace.linux-amd64'
  $launch = Get-GdAgo 240 37
  # The box, then agentspace's linux/amd64 host, then its linux/arm64 host: the amd64 record before the arm64
  # one, so that a guard that sorts, or visits the arm64 host first, shows. Both running, launched long ago.
  $fleetSetup = {
    param([string[]]$Leases)
    Set-HostsRecords @((New-BoxRecord $idBox), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $i2), (New-PfRecord -Project 'agentspace' -InstanceId $i1))
    Set-GdLeases $Leases
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i2 -Launch $launch), (New-GdInstance -Id $i1 -Launch $launch))
  }
  $filter = @($idBox, $i2, $i1)

  # ============ W13: a host is explained by a live lease of its own platform ============
  # Neither platform has a live lease: both hosts are unexplained, past the grace, and stopped.
  Test-GuardCase -Name "${g}: W13: a project with no live lease on either platform has both its hosts stopped after the grace, in the file's order, the amd64 host named by its host name" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $ampName $i2), (Get-GdProgressStopping $armName $i1), (Get-GdClosing 3 2 2 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-CallStop $i1))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $launch), (Get-GdLineStopped $armName $launch)) } } -Setup { & $fleetSetup @() }
  # Only the arm64 platform has a live lease: its host is explained, the amd64 host is not, and is stopped.
  $exp = Get-GdAhead 90 11
  Test-GuardCase -Name "${g}: W13: a project whose only live lease is on linux/arm64 leaves its linux/amd64 host unexplained, logged and stopped, and its arm64 host untouched" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $ampName $i2), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $launch))
      Get-GdSendProblems -Expected @{ $i1 = $exp } -Run $run -Started $started } } -Setup { & $fleetSetup @((New-PfLease agentspace ready -Expires $exp)) }
  # Only the amd64 platform has a live lease: the reverse.
  $expAmd = Get-GdAhead 150 22
  Test-GuardCase -Name "${g}: W13: a project whose only live lease is on linux/amd64 leaves its linux/arm64 host unexplained, logged and stopped, and its amd64 host untouched" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $armName $i1), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i1), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $armName $launch))
      Get-GdSendProblems -Expected @{ $i2 = $expAmd } -Run $run -Started $started } } -Setup { & $fleetSetup @((New-PfLease agentspace ready -Platform 'linux/amd64' -Expires $expAmd)) }
  # A live lease on each platform: both explained, nothing stopped, each sent its own horizon (W14's too).
  $expArmBoth = Get-GdAhead 90 11
  $expAmdBoth = Get-GdAhead 200 33
  Test-GuardCase -Name "${g}: W13: a live lease on each platform explains each host: no stop, no log line, and each host is sent the horizon of its own platform's lease, an arm64 host's never lifted by the amd64 lease of its project, nor the reverse (W14)" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdClosing 3 2 0 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-GdCallSend $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -None
      Get-GdSendProblems -Expected @{ $i1 = $expArmBoth; $i2 = $expAmdBoth } -Run $run -Started $started } } -Setup {
    & $fleetSetup @((New-PfLease agentspace ready -Expires $expArmBoth), (New-PfLease agentspace ready -Platform 'linux/amd64' -Expires $expAmdBoth))
  }
  # A pending lease of one platform is live: it explains that platform's host, whose horizon is the run's time.
  Test-GuardCase -Name "${sLive}: W13: a pending lease on linux/amd64 explains the amd64 host alone, whose horizon is the run's time, and the arm64 host with no lease is stopped" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $armName $i1), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i1), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $armName $launch))
      Get-GdSendProblems -Expected @{ $i2 = 'run' } -Run $run -Started $started } } -Setup { & $fleetSetup @((New-PfLease agentspace pending -Platform 'linux/amd64')) }
  # A released, an expired or an unsatisfiable lease of the platform explains nothing, as for every host, and
  # a released one that was never ready ends at its released_at, counted from that platform alone.
  $relAmd = Get-GdAgo 60 19
  Test-GuardCase -Name "${sLive}: W13: a released and an unsatisfiable lease on linux/amd64 explain nothing: the amd64 host is stopped, counted from the later of its launch and the amd64 leases' last end, while the arm64 host's live lease explains it" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $ampName $i2), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $relAmd))
      Get-GdSendProblems -Expected @{ $i1 = $exp } -Run $run -Started $started } } -Setup {
    & $fleetSetup @((New-PfLease agentspace ready -Expires $exp), (New-PfLease agentspace released -Platform 'linux/amd64' -Released $relAmd), (New-PfLease agentspace unsatisfiable -Platform 'linux/amd64'))
  }
  # An expired ready lease of the platform explains nothing, and its end is its expires_at.
  $expiredAmd = Get-GdAgo 70 41
  Test-GuardCase -Name "${sLive}: W13: a ready lease on linux/amd64 that has expired explains nothing, and counts from its expires_at: the amd64 host is stopped" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $ampName $i2), (Get-GdClosing 3 2 1 0)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $expiredAmd))
      Get-GdSendProblems -Expected @{ $i1 = $exp } -Run $run -Started $started } } -Setup {
    & $fleetSetup @((New-PfLease agentspace ready -Expires $exp), (New-PfLease agentspace ready -Platform 'linux/amd64' -Expires $expiredAmd))
  }
  # A running windows/amd64 host whose project holds only Linux leases is unexplained, and stopped after the
  # grace. The case seeds the record and the running instance and starts nothing Windows.
  $winName = 'winproject.windows-amd64'
  Test-GuardCase -Name "${g}: W13: a running windows/amd64 host whose project holds only Linux leases is unexplained and stopped after the grace, named by its host name; the case seeds a record and an instance and starts nothing Windows" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $winName $i3), (Get-GdClosing 2 1 1 0)))
    Calls = @((Get-GdCallDescribe @($idBox, $i3)), (Get-CallStop $i3))
    Check = { param($run, $started) Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $winName $launch)) } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-PfRecord -Project 'winproject' -Platform 'windows/amd64' -InstanceId $i3))
    Set-GdLeases @((New-PfLease winproject ready -Expires (Get-PfAhead 120)), (New-PfLease winproject ready -Platform 'linux/amd64' -Expires (Get-PfAhead 120)))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i3 -Launch $launch))
  }

  # ============ W14: the log and the progress lines name a host by its host name ============
  Test-GuardCase -Name "${g}: W14: a host off linux/arm64 that fails its part is named host=<project>.linux-amd64 in the log and '<project>.linux-amd64' on stderr, and every linux/arm64 host's line is unchanged: the amd64 instance missing from the answer, the arm64 host sent its horizon" -Expect @{
    Exit = 1
    Stderr = (Join-Lines @((Get-GdHostFailure $ampName "Instance $i2 was not found."), (Get-GdClosing 3 1 0 0), (Get-GdFailedCount 1 3)))
    Calls = @((Get-GdCallDescribe $filter), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineFailed $ampName 'instance' "Instance $i2 was not found."))
      Get-GdSendProblems -Expected @{ $i1 = $exp } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $i2), (New-PfRecord -Project 'agentspace' -InstanceId $i1))
    Set-GdLeases @((New-PfLease agentspace ready -Expires $exp))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i1 -Launch $launch))
  }
  $inside = Get-GdAgo 5 37
  Test-GuardCase -Name "${g}: W14: an unexplained amd64 host inside the grace is logged host=<project>.linux-amd64 unexplained with its stop time and not stopped, sent its launch time as its horizon" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressUnexplained $ampName $i2 $inside), (Get-GdClosing 3 2 0 1)))
    Calls = @((Get-GdCallDescribe $filter), (Get-GdCallSend $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineUnexplained $ampName $inside))
      Get-GdSendProblems -Expected @{ $i2 = $inside; $i1 = $exp } -Run $run -Started $started } } -Setup {
    Set-HostsRecords @((New-BoxRecord $idBox), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $i2), (New-PfRecord -Project 'agentspace' -InstanceId $i1))
    Set-GdLeases @((New-PfLease agentspace ready -Expires $exp))
    Set-GdAws -Instance @($boxStopped, (New-GdInstance -Id $i2 -Launch $inside), (New-GdInstance -Id $i1 -Launch $launch))
  }

  # ============ W15: a host's provision.lock is its own ============
  $lockAmd = Join-Path $fx.Agentspace 'hosts\agentspace.linux-amd64\provision.lock'
  $lockArm = Get-GdProvisionLockPath 'agentspace'
  Test-GuardCase -Name "${g}: W15: the provision.lock of hosts\<project>.linux-amd64\ held leaves the amd64 host unstopped and logged with provisioning=running, still sent its horizon, and the project's arm64 host is stopped" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressProvisioning $ampName $i2), (Get-GdProgressStopping $armName $i1), (Get-GdClosing 3 2 1 1)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i1), (Get-GdCallSend $i2))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineUnexplained $ampName $launch -Provisioning), (Get-GdLineStopped $armName $launch))
      Get-GdSendProblems -Expected @{ $i2 = $launch } -Run $run -Started $started } } -Setup {
    & $fleetSetup @()
    Hold-GdLock $lockAmd
  }
  Test-GuardCase -Name "${g}: W15: the provision.lock of hosts\<project>\ held leaves the arm64 host unstopped and logged with provisioning=running, and the project's amd64 host, whose folder does not exist and reads as free, is stopped, and its folder is not made" -Expect @{
    Exit = 0
    Stderr = (Join-Lines @((Get-GdProgressStopping $ampName $i2), (Get-GdProgressProvisioning $armName $i1), (Get-GdClosing 3 2 1 1)))
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-GdCallSend $i1))
    Check = { param($run, $started)
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $launch), (Get-GdLineUnexplained $armName $launch -Provisioning))
      Get-GdSendProblems -Expected @{ $i1 = $launch } -Run $run -Started $started
      if (Test-Path -LiteralPath (Join-Path $fx.Agentspace 'hosts\agentspace.linux-amd64')) { 'The guard made the folder of a host with no provisioning run' } } } -Setup {
    & $fleetSetup @()
    Hold-GdLock $lockArm
  }
  Test-GuardCase -Name "${g}: W15: a hosts\<project>.linux-amd64\ folder with no lock file reads as free and the lock file is not made: the amd64 host is stopped" -Expect @{
    Exit = 0
    Calls = @((Get-GdCallDescribe $filter), (Get-CallStop $i2), (Get-CallStop $i1))
    Check = { param($run, $started)
      if (Test-Path -LiteralPath $lockAmd) { 'The guard made a lock file to try it' }
      Get-GdLogProblems -Run $run -Started $started -Body @((Get-GdLineStopped $ampName $launch), (Get-GdLineStopped $armName $launch)) } } -Setup {
    & $fleetSetup @()
    New-HomeFolder (Split-Path -Parent $lockAmd)
  }
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's `-Platform` parameter (docs/operator.md, "Command line", "The
# top level", "From the command line"; docs/conformance.md, "v0.12's planned cases for the shared and
# Linux parts", cases W1 to W5; D91, D98; E0.12-T04-S02, card #0262, planned by #0248). Every
# expectation is the spec's text: a message of the table of usage errors, a help block of operator.md,
# or the rule that two command lines act alike.
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp`
# that log every call (conformance/operator/host-commands-common.ps1, platform-common.ps1), a host
# registry the case writes, and the home's throwaway CA. No case starts a real program, calls AWS or
# starts a Windows host.
#
# Cases:
#   W1  `-Platform linux/arm64` changes nothing: for each of the eight commands, for a linux/arm64
#       host and for the box, a run with the flag and a run without it print the same, exit alike,
#       make the same calls in the same order and leave the same files with the same bytes.
#   W2  The value takes every form the lease CLI's --platform takes, and the parameter's name is
#       matched without regard to case.
#   W3  A platform that is not one of the three is refused by each of the eight commands, with the
#       row of the table of usage errors, before any call or file write.
#   W4  `guard` and `ca` refuse -Platform, by check 1a, and check 1 comes first.
#   W5  The command line's other forms: given twice, with no value, a value that looks like a
#       parameter, either side of -Host, -All, no -Host, the rank of the new checks, and every help
#       block that names -Platform, with guard's, byte for byte.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. PATH holds no go. Every instance ID is made
# up, and a failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-platform-flag.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')
. (Join-Path $PSScriptRoot 'platform-common.ps1')

$ErrorActionPreference = 'Stop'

# The help blocks of operator.md, byte for byte as the spec gives them, each from its `Usage:` line to
# the last line before the closing fence (the architect's pin P53 for every block that names -Platform,
# pins P60 and P39 for guard's). The line ending is the spec's: one line feed after every line.
$script:PfHelp = @{}
# top (operator.md lines 2223 to 2267)
$script:PfHelp['top'] = @'
Usage: agentspace-operator <command> [-Host <project> | -All] [-Platform <os/arch>] [-GiB <n>] [-Alias <alias>] [-InstanceType <type>] [-Quiet] [--help]

Manages agentspace's hosts from the laptop: each host's stack, its power, its
system files and toolchain, the SSH CA they trust, and the localswim Bot token
they use. A project has a host on each platform it uses. Without -Host, a
command acts on the box, swimcoach's linux/arm64 host.

Commands:
  deploy     Create or update a host's stack, then run sync
  up         Start a host if needed, wait until it has patched, push the
             Bot token
  sync       Check the CA, install a host's system files, CLAUDE.md and the
             localswim CLI, and push the Bot token
  bootstrap  Install the agent toolchain on a host; safe to run again
  provision  Make a project's host: deploy, start, sync and bootstrap it
  ca         Create the agentspace SSH CA once; later, check it and print it
  down       Stop a host
  status     Show a host's state, type, disk and Session Manager reachability
  grow       Grow a host's disk by -GiB (default 2), and its filesystem with it
  guard      Stop every running host no live lease explains, after a
             15-minute grace, and send each running host its lease horizon

Parameters:
  -Host <project>  The project whose host to act on (default: the box);
                   provision needs it, and ca and guard take none
  -Platform <os/arch>
                   With -Host: that host's platform, linux/arm64 (default),
                   linux/amd64 or windows/amd64; ca, guard and -All take none
  -All             For status, down and sync: every host, one at a time
  -GiB <n>         For grow: the GiB to add (default 2)
  -Alias <alias>   For up: the SSH host alias the token push uses
                   (default: the host's own)
  -InstanceType <type>
                   For deploy: the instance type to give the host
  -Quiet           Print no progress or warnings; failures and status
                   still print
  -h, --help       Print this help, or a command's, and exit

The operator's own messages go to stderr; deploy, sync, bootstrap, provision
and grow pass other programs' output through. up writes nothing to stdout, so
it is safe in an SSH ProxyCommand.

Exit codes:
  0  Success
  1  Failure or usage error; the message on stderr says which
'@
# deploy (operator.md lines 2604 to 2627)
$script:PfHelp['deploy'] = @'
Usage: agentspace-operator deploy [-Host <project>] [-Platform <os/arch>] [-InstanceType <type>] [-Quiet]

Creates or updates a host's CloudFormation stack in us-west-2 from its
template, records the host's instance type in ~/.agentspace/hosts.json, then
runs sync. Without -Host it deploys the box's stack, 'agentbox', with the
parameters in ~/.agentspace/stack-parameters.json, whose InstanceType is
ignored, and your ~/.ssh/id_ed25519.pub. With -Host <project> it deploys that
project's host's stack: when there is none, it creates it with the SubnetId
and VpcId in that file, your public key and the SSH CA's public key; when
there is one, it keeps every parameter but -InstanceType's as it was.

  -Host <project>       The project whose host to deploy (default: the box).
  -Platform <os/arch>   With -Host: the host's platform (default:
                        linux/arm64). A new linux/amd64 host gets the
                        current Ubuntu 26.04 amd64 image, and keeps it.
  -InstanceType <type>  The instance type to give the host, the box included,
                        such as m9g.2xlarge. Without it, a host keeps its
                        type, and a new host gets m9g.xlarge, or m8a.xlarge
                        on linux/amd64.

Never change the template's ImageId on a live stack: it replaces the instance
and its disk.

Exit codes: 0 success, 1 failure.
'@
# up (operator.md lines 2678 to 2697)
$script:PfHelp['up'] = @'
Usage: agentspace-operator up [-Host <project>] [-Platform <os/arch>] [-Alias <alias>] [-Quiet]

Starts a host if it is stopped, waits until its boot-time patch run has
finished, and sends the localswim Bot token and the laptop's time zone if
either has changed since the last confirmed send. Without -Host it acts on
the box.

  -Host <project>  The project whose host to start (default: the box).
  -Platform <os/arch>
                   With -Host: that host's platform (default: linux/arm64).
  -Alias <alias>   The SSH host alias the token push connects with
                   (default: agentspace for the box, agentspace-host.<project>
                   for another host, with .<os>-<arch> after it off
                   linux/arm64). Letters, digits, '.', '_' and '-',
                   not starting with '-'.

Writes nothing to stdout, so it is safe inside an SSH ProxyCommand.
A failed token push warns and still exits 0.

Exit codes: 0 the host is ready, 1 failure.
'@
# sync (operator.md lines 2951 to 2964)
$script:PfHelp['sync'] = @'
Usage: agentspace-operator sync [-Host <project> | -All] [-Platform <os/arch>] [-Quiet]

Checks the agentspace SSH CA, then installs box/system and the CA's public
key on a host, waits until the host has finished this boot's patch run,
copies ~/.claude/CLAUDE.md there, builds the localswim CLI for the host's
platform from C:\Projects\localswim and installs it, and sends the localswim
Bot token and the laptop's time zone. Starts the host first if it is
stopped. Without -Host it acts on the box; -Platform picks the host's
platform (default: linux/arm64); -All syncs every ready host, one at a time.

Fails before touching a host if the CA is missing, half-made, mismatched or
readable by anyone else.

Exit codes: 0 success, 1 failure.
'@
# bootstrap (operator.md lines 3020 to 3028)
$script:PfHelp['bootstrap'] = @'
Usage: agentspace-operator bootstrap [-Host <project>] [-Platform <os/arch>] [-Quiet]

Runs the agent toolchain's bootstrap on a host as its login user: gh, Node
through fnm, Claude Code, Codex, uv, build-essential, and Rust through
rustup. Starts the host first if it is stopped. Safe to run again. Without
-Host it acts on the box; -Platform picks the host's platform (default:
linux/arm64).

Exit codes: 0 success, 1 failure.
'@
# provision (operator.md lines 3480 to 3494)
$script:PfHelp['provision'] = @'
Usage: agentspace-operator provision -Host <project> [-Platform <os/arch>] [-Quiet]

Makes a project's host, or makes it again: deploys its stack, starts it and
waits, runs sync and bootstrap, records it ready in ~/.agentspace/hosts.json,
and places the project's pending leases. Its deploy gives the host the
instance type the largest of those leases needs, unless a ready lease holds
the host. -Platform picks the host's platform (default: linux/arm64). A
project's host on a platform is made only if ~/.agentspace/projects.json
approves that platform for it. One run per host at a time; each run logs to
provision.log in the host's folder under ~/.agentspace/hosts.

Lease commands start a run on their own when a lease needs a host; you can
also run it yourself.

Exit codes: 0 the host is ready, 1 failure.
'@
# ca (operator.md lines 3652 to 3662)
$script:PfHelp['ca'] = @'
Usage: agentspace-operator ca [-Quiet]

Creates the agentspace SSH CA (ed25519, no passphrase) in
%USERPROFILE%\.agentspace\ca the first time, locked to the current user.
Later runs change nothing: they check the key pair and its access rules, and
print its paths and fingerprint.

A half-made, mismatched or loosened CA fails with what is wrong. ca never
repairs or overwrites a CA. Run it without "Run as administrator".

Exit codes: 0 the CA exists and is sound, 1 failure.
'@
# down (operator.md lines 3694 to 3701)
$script:PfHelp['down'] = @'
Usage: agentspace-operator down [-Host <project> | -All] [-Platform <os/arch>] [-Quiet]

Stops a host, or with -All every host. Does not wait for it to stop. Open
sessions on it end. The next connection to it, a lease's or the operator's,
starts it again. Without -Host it stops the box; -Platform picks the host's
platform (default: linux/arm64).

Exit codes: 0 the stop was requested, 1 failure.
'@
# status (operator.md lines 3921 to 3937)
$script:PfHelp['status'] = @'
Usage: agentspace-operator status [-Host <project> | -All] [-Platform <os/arch>] [-Quiet]

Shows a host's instance ID, state, type, root disk size and volume, and
whether Session Manager can reach it; with -All, each host's, after lines
naming it and, off linux/arm64, its platform, then its count of live leases
and, for a running host that no live lease explains, the time the guard
will stop it.
Never starts a host. Prints to stderr. Without -Host it shows the box;
-Platform picks the host's platform (default: linux/arm64).

With -Host, for a host that is not the box, it then signs a fresh one-hour
certificate, so 'ssh agentspace-host.<project>' signs in for the next hour
(agentspace-host.<project>.<os>-<arch> off linux/arm64): run it before you
sign in there, and again after an hour. Sign in under a live lease of the
project on that platform, created first, or the guard stops the host.

Exit codes: 0 success, 1 failure.
'@
# grow (operator.md lines 4009 to 4017)
$script:PfHelp['grow'] = @'
Usage: agentspace-operator grow [-Host <project>] [-Platform <os/arch>] [-GiB <n>] [-Quiet]

Grows a host's root volume by -GiB, a whole number of 1 or more (default 2),
and, if the host is running, its filesystem online; a stopped host grows its
filesystem at next boot. EBS volumes never shrink, and allow one change
every 6 hours. Without -Host it grows the box's; -Platform picks the host's
platform (default: linux/arm64).

Exit codes: 0 success, 1 failure.
'@
# guard (operator.md lines 4435 to 4447)
$script:PfHelp['guard'] = @'
Usage: agentspace-operator guard [-Quiet]

Stops every running host that no live lease explains, once 15 minutes have
passed since the later of its start and its project's last lease end, and
sends every running host its lease horizon, the end of its project's leases
on its platform. Leaves a host whose provisioning run is working on it. Acts
on every host in ~/.agentspace/hosts.json, and takes no -Host or -All. Logs
each stop, each unexplained host and each failure to ~/.agentspace/guard.log.

The keeper runs it every 5 minutes; you can also run it yourself. A guard
that finds another one running exits at once.

Exit codes: 0 success, or another guard is running; 1 failure.
'@
function Get-PfHelpText([string]$Name) { (($script:PfHelp[$Name] -replace "`r`n", "`n").TrimEnd("`n")) + "`n" }

$usageRefusal = 'agentspace: Platform ''{0}'' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64.'
$pairRefusal = 'agentspace: Platform ''{0}'' is not an os/arch pair. Give one as in ''linux/arm64''.'

Invoke-HostFile -FileName 'test-unmet-platform-flag.ps1' -Body {
  param($fx)
  $idA = $script:IdA; $idB = $script:IdB; $idBox = $script:IdBox
  $arm = 'agentspace-host-agentspace'
  $amd = 'agentspace-linux-amd64-host-agentspace'
  # The box, an arm64 host and the same project's amd64 host, each running and long launched, each
  # with its stack answering and its instance ID.
  $seedAll = {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA), (New-PfRecord -Project 'agentspace' -Platform 'linux/amd64' -InstanceId $script:IdB))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA) (Get-HealthyAnswers -Id $script:IdB) (Get-HealthyAnswers -Id $script:IdBox) @{
        'cloudformation describe-stacks agentspace-host-agentspace' = @(New-Step (New-StackDoc -StackName 'agentspace-host-agentspace' -Id $script:IdA))
        'cloudformation describe-stacks agentspace-linux-amd64-host-agentspace' = @(New-Step (New-StackDoc -StackName 'agentspace-linux-amd64-host-agentspace' -Id $script:IdB))
        'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdBox))
      })
  }
  # The same, every instance stopped, so that no command waits for a start or opens an ssh.
  $seedStopped = {
    Set-HostsRecords @((New-BoxRecord), (New-PfRecord -Project 'agentspace' -InstanceId $script:IdA))
    Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdA -State 'stopped') (Get-HealthyAnswers -Id $script:IdBox -State 'stopped') @{
        'cloudformation describe-stacks agentspace-host-agentspace' = @(New-Step (New-StackDoc -StackName 'agentspace-host-agentspace' -Id $script:IdA))
        'cloudformation describe-stacks agentbox' = @(New-Step (New-StackDoc -StackName 'agentbox' -Id $script:IdBox))
      })
  }
  $statusB = { param([string]$Id) Get-StatusLines $Id }
  $platformLine = 'Platform  linux/amd64'

  # ============ W1: -Platform linux/arm64 changes nothing ============
  $s = 'operator.md#command-line'
  $commands = @(
    @{ N = 'deploy'; A = @('deploy'); Seed = $seedAll },
    @{ N = 'up'; A = @('up'); Seed = $seedAll },
    @{ N = 'sync'; A = @('sync'); Seed = $seedAll },
    @{ N = 'down'; A = @('down'); Seed = $seedAll },
    @{ N = 'status'; A = @('status'); Seed = $seedAll },
    @{ N = 'grow -GiB 4'; A = @('grow', '-GiB', '4'); Seed = $seedStopped },
    @{ N = 'bootstrap'; A = @('bootstrap'); Seed = $seedAll },
    @{ N = 'provision'; A = @('provision'); Seed = $seedAll }
  )
  foreach ($c in $commands) {
    $c1 = $c
    Invoke-PfCase "${s}: W1: $($c1.N) -Host <project> -Platform linux/arm64 is the same as no -Platform, for an arm64 host and for the box: the same output, exit code, calls in order and files with the same bytes" {
      param($p)
      foreach ($who in 'agentspace', 'swimcoach') {
        $withOut = Invoke-PfCapture -Arguments (@($c1.A) + @('-Host', $who)) -Setup $c1.Seed
        $withFlag = Invoke-PfCapture -Arguments (@($c1.A) + @('-Host', $who, '-Platform', 'linux/arm64')) -Setup $c1.Seed
        if ($withOut.Aws.Count -eq 0) { $p.Add("The control run for '$who' made no AWS call, so the case shows nothing") }
        foreach ($d in (Get-PfDifference $withOut $withFlag)) { $p.Add("-Host ${who}: $d") }
      }
    }
  }

  # ============ W2: the value takes every form the lease CLI's --platform takes ============
  $s = 'operator.md#command-line'
  $fiveB = Get-StatusLines $idB
  $fiveA = Get-StatusLines $idA
  $callsB = @((Get-CallDescribe $idB), (Get-CallVolumes $idB), (Get-CallSsm $idB))
  $callsA = @((Get-CallDescribe $idA), (Get-CallVolumes $idA), (Get-CallSsm $idA))
  foreach ($form in @(' Linux/AMD64 ', 'linux/x86_64', 'LINUX/X86-64', 'linux/amd64')) {
    $f2 = $form
    Test-HostCase "${s}: W2: -Platform '$f2' acts as linux/amd64: the amd64 host's calls, the Platform line with the normalized value, and the amd64 name part in the alias file" @('status', '-Host', 'agentspace', '-Platform', $f2) @{
      Exit = 0; Stderr = ($platformLine + "`n" + $fiveB); Calls = $callsB
      Check = { param($run, $started)
        Get-PfSignProblems 'agentspace' 'linux/amd64'
        if (Test-Path -LiteralPath (Get-PfFiles 'agentspace').Alias) { 'The arm64 host''s alias file was written' }
        if ((Get-PfAlias 'agentspace' 'linux/amd64') -cne 'agentspace-host.agentspace.linux-amd64') { 'The spec''s alias for the amd64 host is agentspace-host.agentspace.linux-amd64' }
      } } -Setup $seedAll
  }
  foreach ($form in @('linux/aarch64', 'linux/AARCH64/v8', 'linux/arm64/v8', ' LINUX/Arm64 ')) {
    $f2 = $form
    Test-HostCase "${s}: W2: -Platform '$f2' acts as linux/arm64, as the run without the flag does: the arm64 host's five lines, calls and alias" @('status', '-Host', 'agentspace', '-Platform', $f2) @{
      Exit = 0; Stderr = $fiveA; Calls = $callsA
      Check = { param($run, $started)
        Get-PfSignProblems 'agentspace' 'linux/arm64'
        if (Test-Path -LiteralPath (Get-PfFiles 'agentspace' 'linux/amd64').Alias) { 'The amd64 host''s alias file was written' }
      } } -Setup $seedAll
  }
  foreach ($name in @('-platform', '-PLATFORM', '-PlAtFoRm')) {
    $n2 = $name
    Test-HostCase "${s}: W2: the parameter's name is matched without regard to case: $n2 linux/amd64" @('status', '-Host', 'agentspace', $n2, 'linux/amd64') @{
      Exit = 0; Stderr = ($platformLine + "`n" + $fiveB); Calls = $callsB } -Setup $seedAll
  }

  # ============ W3: a platform that is not one of the three is refused ============
  $s = 'operator.md#command-line'
  $eight = @(
    @('deploy'), @('up'), @('sync'), @('bootstrap'), @('provision'), @('down'), @('status'), @('grow')
  )
  $values = @(
    @{ Arg = 'windows/arm64'; Want = ($usageRefusal -f 'windows/arm64') },
    @{ Arg = 'Linux/RiscV64'; Want = ($usageRefusal -f 'linux/riscv64') },
    @{ Arg = 'linux/amd64/v2'; Want = ($usageRefusal -f 'linux/amd64/v2') },
    @{ Arg = 'linux/x86_64/v3'; Want = ($usageRefusal -f 'linux/amd64/v3') },
    @{ Arg = 'darwin/arm64'; Want = ($usageRefusal -f 'darwin/arm64') },
    @{ Arg = 'linux'; Want = ($pairRefusal -f 'linux') },
    @{ Arg = 'linux-amd64'; Want = ($pairRefusal -f 'linux-amd64') },
    @{ Arg = 'windows'; Want = ($pairRefusal -f 'windows') },
    @{ Arg = ' Linux '; Want = ($pairRefusal -f ' Linux ') },
    @{ Arg = ''; Want = ($pairRefusal -f '') }
  )
  foreach ($v in $values) {
    $v2 = $v
    $shown = if ($v2.Arg -eq '') { '(an empty value)' } else { "'$($v2.Arg)'" }
    Invoke-PfCase "${s}: W3: -Platform $shown is refused by each of the eight commands with -Host <project>: exit 1, nothing on stdout, the table's row as the last line of stderr, no program started and nothing written in the home" {
      param($p)
      Reset-HostCase
      & $seedAll
      Clear-StandinLogs
      $before = Get-PfSnapshot
      foreach ($cmd in $eight) {
        $r = Invoke-HostRun -Arguments (@($cmd) + @('-Host', 'agentspace', '-Platform', $v2.Arg))
        $name = $cmd[0]
        if ($r.TimedOut) { $p.Add("${name}: it timed out"); continue }
        if ($r.ExitCode -ne 1) { $p.Add("${name}: exit $($r.ExitCode), not 1") }
        if ($r.Stdout.Length -gt 0) { $p.Add("${name}: stdout is not empty") }
        $last = @($r.Stderr -split "`n" | Where-Object { $_ -ne '' }) | Select-Object -Last 1
        if ($last -cne $v2.Want) { $p.Add("${name}: the last line of stderr is not the table's row '$(Format-QuotedSpec $v2.Want)'") }
      }
      foreach ($k in @(@('AWS', (Get-AwsCalls)), @('ssh', (Get-SshCalls)), @('scp', (Get-ScpCalls)))) { if (@($k[1]).Count -gt 0) { $p.Add("$($k[0]) was called $(@($k[1]).Count) times") } }
      if ((Get-PfSnapshot) -cne $before) { $p.Add('The home''s files changed: a key, a certificate, an alias, a registry write or an operator folder') }
    }
  }
  Invoke-PfCase "${s}: W3: the refusal is made before the host is looked up: a project with no host on any platform gets the platform's row, not the no-host message" {
    param($p)
    Reset-HostCase
    $r = Invoke-HostRun -Arguments @('status', '-Host', 'nosuchproject', '-Platform', 'windows/arm64')
    $last = @($r.Stderr -split "`n" | Where-Object { $_ -ne '' }) | Select-Object -Last 1
    if ($r.ExitCode -ne 1) { $p.Add("Exit $($r.ExitCode), not 1") }
    if ($last -cne ($usageRefusal -f 'windows/arm64')) { $p.Add('The last line of stderr is not the table''s row') }
  }

  # ============ W4: guard and ca refuse -Platform ============
  $s = 'operator.md#command-line'
  foreach ($c in @(
      @{ A = @('guard', '-Platform', 'linux/amd64'); Want = 'agentspace: -Platform does not apply to guard.'; N = 'guard -Platform linux/amd64 is refused by check 1a' },
      @{ A = @('ca', '-Platform', 'linux/amd64'); Want = 'agentspace: -Platform does not apply to ca.'; N = 'ca -Platform linux/amd64 is refused by check 1a' },
      @{ A = @('guard', '-Platform', 'linux/arm64'); Want = 'agentspace: -Platform does not apply to guard.'; N = 'guard -Platform linux/arm64 is refused too, a flag that does not apply being refused whatever it holds' },
      @{ A = @('guard', '-Host', 'x', '-Platform', 'linux/amd64'); Want = 'agentspace: -Host does not apply to guard.'; N = 'guard -Host x -Platform linux/amd64 reports check 1, which comes first' },
      @{ A = @('ca', '-Host', 'x', '-Platform', 'linux/amd64'); Want = 'agentspace: -Host does not apply to ca.'; N = 'ca -Host x -Platform linux/amd64 reports check 1, which comes first' },
      @{ A = @('guard', '-Platform', 'linux/amd64', '-All'); Want = 'agentspace: -Platform does not apply to guard.'; N = 'guard -Platform linux/amd64 -All reports check 1a, before checks 2 and 3a' })) {
    $c2 = $c
    Test-HostCase "${s}: W4: $($c2.N), exit 1, byte for byte, no program started" $c2.A @{ Exit = 1; Stderr = ($c2.Want + "`n"); Calls = @() } -Setup $seedAll
  }

  # ============ W5: the command line's other forms ============
  $s = 'operator.md#command-line'
  Test-HostCase "${s}: W5: -Platform given twice is refused, even with the same value" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64', '-Platform', 'linux/amd64') @{
    Exit = 1; Stderr = "agentspace: -Platform is given more than once.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: -Platform given with no value, as the last argument, is worded as -Host's is" @('status', '-Host', 'agentspace', '-Platform') @{
    Exit = 1; Stderr = "agentspace: -Platform needs a value.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: a repeated name is found before a missing value: -Platform linux/amd64 -Platform" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64', '-Platform') @{
    Exit = 1; Stderr = "agentspace: -Platform is given more than once.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: a value that looks like a parameter is the value: -Platform -Quiet is refused as not an os/arch pair" @('status', '-Host', 'agentspace', '-Platform', '-Quiet') @{
    Exit = 1; Stderr = (($pairRefusal -f '-Quiet') + "`n"); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: a value that looks like a parameter is the value: -Platform -All is refused as not an os/arch pair" @('status', '-Host', 'agentspace', '-Platform', '-All') @{
    Exit = 1; Stderr = (($pairRefusal -f '-All') + "`n"); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: help is checked first, and -h is the one value that is not taken: status -Host agentspace -Platform -h prints status's help" @('status', '-Host', 'agentspace', '-Platform', '-h') @{
    Exit = 0; Stdout = (Get-PfHelpText 'status'); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: -Platform before -Host acts as after it" @('status', '-Platform', 'linux/amd64', '-Host', 'agentspace') @{
    Exit = 0; Stderr = ($platformLine + "`n" + $fiveB); Calls = $callsB } -Setup $seedAll
  Test-HostCase "${s}: W5: -Platform after -Host acts as before it" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64') @{
    Exit = 0; Stderr = ($platformLine + "`n" + $fiveB); Calls = $callsB } -Setup $seedAll
  foreach ($cmd in 'status', 'down', 'sync') {
    $c3 = $cmd
    Test-HostCase "${s}: W5: $c3 -All -Platform linux/amd64 is refused by check 3a" @($c3, '-All', '-Platform', 'linux/amd64') @{
      Exit = 1; Stderr = "agentspace: -Platform and -All cannot be given together.`n"; Calls = @() } -Setup $seedAll
  }
  Test-HostCase "${s}: W5: -Platform before -All is refused by check 3a as well" @('status', '-Platform', 'linux/amd64', '-All') @{
    Exit = 1; Stderr = "agentspace: -Platform and -All cannot be given together.`n"; Calls = @() } -Setup $seedAll
  foreach ($cmd in 'deploy', 'up', 'sync', 'bootstrap', 'down', 'status', 'grow') {
    $c3 = $cmd
    Test-HostCase "${s}: W5: $c3 -Platform linux/amd64 with no -Host is refused by check 4a, the box's included" @($c3, '-Platform', 'linux/amd64') @{
      Exit = 1; Stderr = "agentspace: -Platform needs -Host <project>.`n"; Calls = @() } -Setup $seedAll
  }
  Test-HostCase "${s}: W5: -Platform linux/arm64 with no -Host is refused by check 4a too, for the box" @('status', '-Platform', 'linux/arm64') @{
    Exit = 1; Stderr = "agentspace: -Platform needs -Host <project>.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: provision -Platform linux/amd64 with no -Host is refused by check 4, which comes first" @('provision', '-Platform', 'linux/amd64') @{
    Exit = 1; Stderr = "agentspace: provision needs -Host <project>.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank: status -All -Platform linux/amd64 -InstanceType m8a.large reports check 3a, before check 5" @('status', '-All', '-Platform', 'linux/amd64', '-InstanceType', 'm8a.large') @{
    Exit = 1; Stderr = "agentspace: -Platform and -All cannot be given together.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank: status -Platform linux/amd64 -InstanceType m8a.large reports check 4a, before check 5" @('status', '-Platform', 'linux/amd64', '-InstanceType', 'm8a.large') @{
    Exit = 1; Stderr = "agentspace: -Platform needs -Host <project>.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank: status -Host agentspace -Platform linux/amd64 -InstanceType m8a.large reaches check 5, once -Platform has its -Host" @('status', '-Host', 'agentspace', '-Platform', 'linux/amd64', '-InstanceType', 'm8a.large') @{
    Exit = 1; Stderr = "agentspace: -InstanceType applies to deploy alone.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank, left to right: -Host's bad value found before -Platform's, with -Host first" @('status', '-Host', 'bad!', '-Platform', 'windows/arm64') @{
    Exit = 1; Stderr = "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not 'bad!'.`n"; Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank, left to right: -Platform's bad value found before -Host's, with -Platform first" @('status', '-Platform', 'windows/arm64', '-Host', 'bad!') @{
    Exit = 1; Stderr = (($usageRefusal -f 'windows/arm64') + "`n"); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank, left to right: -Platform's bad value found before -GiB's, with -Platform first" @('status', '-Host', 'agentspace', '-Platform', 'windows/arm64', '-GiB', 'abc') @{
    Exit = 1; Stderr = (($usageRefusal -f 'windows/arm64') + "`n"); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: rank, left to right: -GiB's bad value found before -Platform's, with -GiB first" @('status', '-Host', 'agentspace', '-GiB', 'abc', '-Platform', 'windows/arm64') @{
    Exit = 1; Stderr = "agentspace: -GiB MUST be a whole number, not 'abc'.`n"; Calls = @() } -Setup $seedAll

  # The help blocks, byte for byte: stdout is the block with a line feed after every line and no carriage return.
  $s = 'operator.md#command-line'
  foreach ($name in 'deploy', 'up', 'sync', 'bootstrap', 'provision', 'down', 'status', 'grow', 'guard', 'ca') {
    $n2 = $name
    foreach ($flag in '--help', '-h') {
      $f2 = $flag
      Test-HostCase "${s}: W5: $n2 $f2 prints its help block byte for byte, with a line feed after every line and no carriage return" @($n2, $f2) @{
        Exit = 0; Stdout = (Get-PfHelpText $n2); Calls = @()
        Check = { param($run, $started) if ($run.Stdout.Contains("`r")) { 'The help holds a carriage return' } } } -Setup $seedAll
    }
  }
  Test-HostCase "${s}: W5: the top level's help, byte for byte, for --help" @('--help') @{
    Exit = 0; Stdout = (Get-PfHelpText 'top'); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: the top level's help, byte for byte, for -h" @('-h') @{
    Exit = 0; Stdout = (Get-PfHelpText 'top'); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: help asked with -Platform on the line prints the command's help: sync -Platform linux/amd64 --help" @('sync', '-Platform', 'linux/amd64', '--help') @{
    Exit = 0; Stdout = (Get-PfHelpText 'sync'); Calls = @() } -Setup $seedAll
  Test-HostCase "${s}: W5: guard's help names the end of its project's leases on its platform, and no -Host, -Platform or -All it does not take" @('guard', '--help') @{
    Exit = 0; Stdout = (Get-PfHelpText 'guard'); Calls = @()
    Check = { param($run, $started)
      if (-not $run.Stdout.Contains("the end of its project's leases`non its platform")) { 'The help does not say the horizon is the end of its project''s leases on its platform' }
      if ($run.Stdout -cmatch '-Platform <') { 'guard''s help names -Platform as a parameter it takes' }
    } } -Setup $seedAll
}

Exit-ConformanceRun

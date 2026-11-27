#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of `grow`'s refusal of a -GiB of 0 or less (docs/operator.md, "grow", "Command line",
# "Exit codes", "Calls by command"; D61, #0164; P7, architect, 2026-11-21, for v0.08-T01). Card #0182
# (E0.08-T04-S01). It stands beside test-unmet-commands.ps1's two cases from before v0.08, `-GiB 0` (which the
# AWS stand-in's model of EC2's refusal served) and `-GiB -1` (which asked EC2 for the size less 1), which
# E0.08-T03-S02 (#125) rewrote to the refusal and which stay beside these cases (operator.md, Which checks
# show it; the overlap kept by the architect, 2026-11-22).
#
# What it does: it runs the operator in a throwaway home with stand-ins for `aws`, `ssh` and `scp` that
# log every call and answer from files beside them (conformance/operator/host-commands-common.ps1), over a
# registry of the box and a host, both healthy. The stand-in answers a modify-volume for any size, as the
# AWS CLI does for a size that grows the volume, and models no refusal: a build that goes on to EC2 with a
# size of 0 or less is caught by the calls the stand-in logs, never by an answer it was told to give.
#
# Cases: -GiB 0, -0, +0, 000, -1, -5, -007 and -2147483648, each giving exactly the refusal on stderr,
# nothing on stdout, exit 1, no AWS call at all (no describe-instances, no describe-volumes, and no
# modify-volume counted), no ssh and no scp, and nothing written anywhere in the home; the same with -Host
# for a host that is not the box, with no key, certificate or alias written; -GiB 1 asking EC2 for the
# size plus 1; the refusal's place in the order of checks (the -Alias, -All and -InstanceType messages, the
# usage errors of the arguments, a -Host with no record, -Quiet, and help, each in front of or behind it);
# and `status`, `down` and `sync` not refusing a -GiB of 0 or less, which only `grow` does.
#
# NOT SHOWN HERE. operator.md's coverage table also names `grow -GiB 0` with USERPROFILE unset. A check
# MUST NOT run a program with USERPROFILE unset or empty (docs/conformance.md, "The harness"), and the
# harness refuses that override, so that order is shown by the operator crate's own tests, as operator.md
# (27643b2) names: the test `grows_gib_refusal_is_a_usage_error_that_needs_neither`, in tests/home_and_system_root.rs
# of the operator crate, which shows the refusal needs neither USERPROFILE nor SystemRoot.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential. Before any program that can reach AWS starts, the check confirms under
# the same environment that `aws sts get-caller-identity` fails for every AWS CLI the operator can run,
# and reports SKIPPED, running nothing, if it does not. Every instance and volume ID is made up, and a
# failure reason never quotes the program's output. Every write and delete goes through
# Assert-ConformancePath at the call.
#
# The program under test is the one AGENTSPACE_OPERATOR_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-grow-zero.ps1
. (Join-Path $PSScriptRoot 'host-commands-common.ps1')

$ErrorActionPreference = 'Stop'
$script:HomeBefore = $null

# Every file and folder under the throwaway home, with each file's size and time, as one text.
function Get-HomeTree {
  $root = $script:Fx.H.Root
  (@(Get-ChildItem -LiteralPath $root -Recurse -Force | Sort-Object FullName | ForEach-Object {
        $rel = $_.FullName.Substring($root.Length)
        if ($_.PSIsContainer) { "d $rel" } else { "f $rel $($_.Length) $($_.LastWriteTimeUtc.Ticks)" }
      }) -join "`n")
}
function Save-HomeTree { $script:HomeBefore = Get-HomeTree }
# `grow`'s refusal writes nothing: no file or folder made, changed or removed in the home.
function Test-HomeUnchanged {
  if ($null -ne $script:HomeBefore -and (Get-HomeTree) -cne $script:HomeBefore) { 'Something in the home was made, changed or removed' }
}
function Set-TwoHosts {
  Set-HostsRecords @((New-BoxRecord), (New-HostRecord -Project 'agentspace' -InstanceId $script:IdA))
  Set-AwsScenario (Merge-Scenario (Get-HealthyAnswers -Id $script:IdBox -State 'stopped') (Get-HealthyAnswers -Id $script:IdA -State 'stopped'))
  Save-HomeTree
}

Invoke-HostFile -FileName 'test-unmet-grow-zero.ps1' -Body {
  param($fx)
  $idBox = $script:IdBox
  $g = 'operator.md#grow'
  $c = 'operator.md#command-line'
  $x = 'operator.md#exit-codes'
  $unchanged = { param($run, $started) Test-HomeUnchanged; Get-NothingSignedProblems }
  $refusal = { param([string]$Arg) "agentspace: -GiB MUST be 1 or more for grow, not '$Arg'.`n" }

  # ============ The refusal ============
  foreach ($v in '0', '-0', '+0', '000', '-1', '-5', '-007', '-2147483648') {
    Test-HostCase "${g}: grow -GiB $v is refused as a usage error: exactly the message on stderr, nothing on stdout, exit 1, no AWS call at all (no describe-instances, no describe-volumes, no modify-volume), no ssh, nothing written in the home" @('grow', '-GiB', $v) @{
      Exit = 1; Stderr = (& $refusal $v); Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  }
  Test-HostCase "${g}: grow -Host <project> -GiB 0 of a host that is not the box is refused the same way, and writes no key, no certificate and no alias, with no AWS call" @('grow', '-Host', 'agentspace', '-GiB', '0') @{
    Exit = 1; Stderr = (& $refusal '0'); Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${g}: grow -Host <project> -GiB -1 of a host that is not the box is refused the same way, with nothing signed and nothing written" @('grow', '-Host', 'agentspace', '-GiB', '-1') @{
    Exit = 1; Stderr = (& $refusal '-1'); Calls = @(); SshCalls = @(); ScpCalls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${g}: -Quiet does not withhold the refusal, which is a failure" @('grow', '-GiB', '0', '-Quiet') @{
    Exit = 1; Stderr = (& $refusal '0'); Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${g}: the refusal is the same whatever the case of the parameter name (-gib)" @('grow', '-gib', '-3') @{
    Exit = 1; Stderr = (& $refusal '-3'); Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }

  # ============ The boundary ============
  Test-HostCase "${g}: grow -GiB 1 is not refused: it asks EC2 for the volume's size plus 1 (21 for a 20 GiB volume), and a stopped box grows its filesystem at next boot" @('grow', '-GiB', '1') @{
    Exit = 0
    Stderr = (Join-Lines @("Growing $(Get-VolumeOf $idBox) from 20 to 21 GiB (EBS allows one change per volume every 6 hours)...", 'The filesystem grows on its own at next boot.'))
    Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox), (Get-CallModify $idBox 21)) } -Setup { Set-TwoHosts }

  # ============ The refusal's place in the order of checks ============
  Test-HostCase "${c}: -Alias is found before the refusal: grow -Alias x -GiB 0 gives -Alias applies to up alone., and no AWS call" @('grow', '-Alias', 'x', '-GiB', '0') @{
    Exit = 1; Stderr = "agentspace: -Alias applies to up alone.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: -All is found before the refusal: grow -All -GiB 0 gives -All applies to status, down and sync alone." @('grow', '-All', '-GiB', '0') @{
    Exit = 1; Stderr = "agentspace: -All applies to status, down and sync alone.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: -InstanceType is found before the refusal: grow -InstanceType m9g.xlarge -GiB 0 gives -InstanceType applies to deploy alone." @('grow', '-InstanceType', 'm9g.xlarge', '-GiB', '0') @{
    Exit = 1; Stderr = "agentspace: -InstanceType applies to deploy alone.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: the refusal comes before the host registry is read: grow -Host nobody -GiB 0 gives the refusal, not the project's missing host" @('grow', '-Host', 'nobody', '-GiB', '0') @{
    Exit = 1; Stderr = (& $refusal '0'); Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: an argument's usage error is found first: grow -GiB 0 -GiB 1 gives -GiB is given more than once." @('grow', '-GiB', '0', '-GiB', '1') @{
    Exit = 1; Stderr = "agentspace: -GiB is given more than once.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: an argument's usage error is found first: grow -GiB 0 extra gives Unexpected argument 'extra'." @('grow', '-GiB', '0', 'extra') @{
    Exit = 1; Stderr = "agentspace: Unexpected argument 'extra'.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: a -Host value that is not a project name is found first: grow -Host Bad! -GiB 0 gives the -Host message" @('grow', '-Host', 'Bad!', '-GiB', '0') @{
    Exit = 1; Stderr = "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not 'Bad!'.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: a -GiB that is not a whole number is the -GiB form's usage error, not the refusal: grow -GiB 0x1 gives -GiB MUST be a whole number" @('grow', '-GiB', '0x1') @{
    Exit = 1; Stderr = "agentspace: -GiB MUST be a whole number, not '0x1'.`n"; Calls = @(); Check = $unchanged } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: help is checked first: grow -GiB 0 -h prints grow's help to stdout and exits 0, with no AWS call" @('grow', '-GiB', '0', '-h') @{
    Exit = 0; StdoutAny = $true; Calls = @()
    Check = { param($run, $started)
      if (-not $run.Stdout.StartsWith("Usage: agentspace-operator grow [-Host <project>] [-Platform <os/arch>] [-GiB <n>] [-Quiet]`n")) { 'Stdout is not grow''s help' }
      if ($run.Stderr.Trim().Length -gt 0) { 'Stderr is not empty' }
      Test-HomeUnchanged
    } } -Setup { Set-TwoHosts }
  Test-HostCase "${x}: the refusal is a usage error like every other, exit 1 and not another code: grow -GiB -2147483648 exits 1" @('grow', '-GiB', '-2147483648') @{
    Exit = 1; Stderr = (& $refusal '-2147483648'); Calls = @() } -Setup { Set-TwoHosts }

  # ============ No other command checks the value ============
  Test-HostCase "${c}: status -GiB 0 is not refused: -GiB's form is checked and its value ignored, so the five lines print and the exit is 0" @('status', '-GiB', '0') @{
    Exit = 0; Stderr = (Get-StatusLines $idBox -State 'stopped' -Ssm '-'); Calls = @((Get-CallDescribe $idBox), (Get-CallVolumes $idBox)) } -Setup { Set-TwoHosts }
  Test-HostCase "${c}: down -GiB -5 is not refused: the stop is requested, and the exit is 0" @('down', '-GiB', '-5') @{
    Exit = 0; Stderr = (Join-Lines @("Stopping $idBox.")); Calls = @((Get-CallStop $idBox)) } -Setup { Set-TwoHosts }
  # sync's own steps are not this check's: it only has to go past the command line, so the check is that
  # the refusal's message is nowhere in what it prints, that no usage error was printed, and that the program
  # reached the AWS stand-in at least once, so that the check is not met by a program that stopped early.
  $syncName = "${c}: sync -GiB 0 is not refused: its stderr never holds the refusal's message (its own steps are another file's)"
  try {
    Reset-HostCase
    Set-TwoHosts
    $run = Invoke-HostRun -Arguments @('sync', '-GiB', '0') -Timeout 180
    $why = [System.Collections.Generic.List[string]]::new()
    if ($run.TimedOut) { $why.Add('It timed out') }
    if ($run.Stderr -match 'MUST be 1 or more') { $why.Add('Stderr holds the grow refusal') }
    if ($run.Stderr -cmatch '(?m)^agentspace: (No command given|Unknown command|Unexpected argument|Unknown parameter|-\w+ is given more than once|-\w+ needs a value|-GiB MUST be)') { $why.Add('It was refused as a usage error') }
    if (@(Get-AwsCalls).Count -eq 0) { $why.Add('It never reached the AWS CLI, so the command line was not accepted') }
    if ($run.Stderr -match 'CategoryInfo|At line:|Exception|panicked') { $why.Add('Stderr shows a runtime error record') }
    if ($why.Count -eq 0) { Write-ConformanceResult -Name $syncName -Status PASS } else { Write-ConformanceResult -Name $syncName -Status FAIL -Reason (ConvertTo-ConformanceMasked ($why -join '; ')) }
  } catch {
    $msg = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
    Write-ConformanceResult -Name $syncName -Status FAIL -Reason "The check hit an error: $msg"
  }
}
Exit-ConformanceRun

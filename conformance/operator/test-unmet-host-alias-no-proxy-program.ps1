#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the operator's alias for a host when the ProxyCommand program is not beside the
# operator (docs/operator.md, "How the operator reaches a host", step 4, the bullet "A program that is
# not there"; "How each program finds the others"; "Which checks show it", the row for the operator's
# alias refused when agentspace-ssh-proxy.exe is not beside the operator), run through the test seam.
# Story E0.07-T04-S02 (#0156).
#
# The check copies the operator under test, alone, into a folder under the throwaway home and runs it
# from there with `bootstrap -Host other`, whose step 2 makes the host's alias, and whose step 3 would
# start an ssh. It shows that:
#   - the command fails, exit 1, with `agentspace: <path> is missing; the operator's alias for a host
#     needs it beside the operator.`, where <path> is agentspace-ssh-proxy.exe in the operator's own
#     folder, writes no alias file, and starts no ssh (the stand-in ssh is a tripwire);
#   - only the .exe is looked for: a forwarder agentspace-ssh-proxy.cmd beside the operator does not
#     meet the need, and the same failure follows;
#   - with agentspace-ssh-proxy.exe copied beside the operator the same command succeeds and the alias's
#     ProxyCommand names that copy, in the operator's own folder, then the alias (the control that
#     the failure above is the missing program's).
#
# A copy of an installed program is never made: when the operator or the ProxyCommand program under
# test is the one in the real profile's .agentspace folder (no AGENTSPACE_OPERATOR_CMD set), the check reports SKIPPED, since
# AGENTS.md ("Terry's real files in a review") keeps a check from reading that folder.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA, APPDATA and the ssh folder into the
# throwaway home and removes every AWS credential, and the check confirms first that `aws sts
# get-caller-identity` fails under that environment for every AWS CLI the operator can run (SKIPPED if
# not). The copy runs under the harness's environment. A failure reason never quotes the program's output.
#
# The programs under test are the ones AGENTSPACE_OPERATOR_CMD and AGENTSPACE_SSH_PROXY_CMD name; unset,
# each takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-alias-no-proxy-program.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'operator.md#how-the-operator-reaches-a-host'
$project = 'other'
$aliasName = "agentspace-host.$project"

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the operator's alias for a host needs the ProxyCommand program beside the operator"
  $operator = Get-ConformanceProgram -Variable 'AGENTSPACE_OPERATOR_CMD'
  $proxy = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $realAgentspace = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.agentspace'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $operator -or -not $proxy) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither a conformance variable nor its default names the operator or the ProxyCommand program'
  } elseif (([IO.Path]::GetFullPath($operator).StartsWith($realAgentspace + '\', [StringComparison]::OrdinalIgnoreCase)) -or ([IO.Path]::GetFullPath($proxy).StartsWith($realAgentspace + '\', [StringComparison]::OrdinalIgnoreCase))) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason 'The operator or the ProxyCommand program under test is the installed one, which this check does not copy; set AGENTSPACE_OPERATOR_CMD and AGENTSPACE_SSH_PROXY_CMD to a build'
  } elseif (-not $h.CaReady) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $h.CaReason
  } else {
    $S = New-HpStandIns $h
    $sshOk = New-OperatorStandIn -ConformanceHome $h -Name 'ssh-ok' -Body @('findstr "^" >nul', 'exit /b 0')
    Set-HpRegistry $h @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-HpHostRecord $project 'ready' $HpIdOther))
    $copyDir = Join-Path $h.Root 'operator-alone'
    $copy = Join-Path $copyDir 'agentspace-operator.exe'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path $copyDir) | Out-Null
    Copy-Item -LiteralPath $operator -Destination (Assert-ConformancePath -ConformanceHome $h -Path $copy)
    $aliasPath = Join-Path $h.SshDir "agentspace.d\$aliasName"
    $expectProgram = Join-Path $copyDir 'agentspace-ssh-proxy.exe'

    function Invoke-Copy([string]$Ssh) {
      $e = @{ AGENTSPACE_AWS_CMD = $S.Aws; AGENTSPACE_SSH_CMD = $Ssh; AGENTSPACE_SCP_CMD = $S.Scp }
      $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $h -Override $e
      Start-ConformanceProcess -FilePath $copy -Arguments @('bootstrap', '-Host', $project) -ChildEnvironment $childEnv -TimeoutSeconds 180
    }
    function Test-Missing($Run, [string]$Label) {
      $p = New-HpProblems
      if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
      if ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
      $m = @($Run.Stderr -split "`r?`n" | Where-Object { $_ -cmatch "\Aagentspace: .+ is missing; the operator's alias for a host needs it beside the operator\.\z" })
      if ($m.Count -eq 0) { $p.Add('Stderr lacks "agentspace: <path> is missing; the operator''s alias for a host needs it beside the operator."') }
      else {
        $named = ($m[0] -replace '\Aagentspace: (.+) is missing; .*\z', '$1')
        if ($named -ine $expectProgram) { $p.Add('The path named is not agentspace-ssh-proxy.exe in the operator''s own folder') }
      }
      if (Test-Path -LiteralPath $aliasPath) { $p.Add('An alias file was written') }
      if ((Get-StandInCallCount -Log (Join-Path $S.Dir 'ssh-tripwire-calls.log')) -ne 0 -or (Get-StandInCallCount -Log (Join-Path $S.Dir 'ssh-ok-calls.log')) -ne 0) { $p.Add('ssh was started') }
      , $p
    }

    $r = Invoke-Copy $S.Ssh
    Write-HpProblems "${sec}: With agentspace-ssh-proxy.exe not beside the operator, the command fails with the missing-program message, writes no alias and starts no ssh" (Test-Missing $r 'alone')

    Set-HpFile $h (Join-Path $copyDir 'agentspace-ssh-proxy.cmd') "@echo off`r`nexit /b 1`r`n"
    $r = Invoke-Copy $S.Ssh
    Write-HpProblems "${sec}: Only the .exe is looked for: a forwarder .cmd beside the operator does not meet the need" (Test-Missing $r 'cmd')

    Copy-Item -LiteralPath $proxy -Destination (Assert-ConformancePath -ConformanceHome $h -Path $expectProgram)
    $r = Invoke-Copy $sshOk
    $p = New-HpProblems
    if ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
    if ((Get-StandInCallCount -Log (Join-Path $S.Dir 'ssh-ok-calls.log')) -ne 1) { $p.Add('ssh was not started once') }
    $text = Get-HpText $aliasPath
    if ($null -eq $text) { $p.Add('No alias file was written') }
    else {
      $line = @($text -split "`n" | Where-Object { $_ -cmatch '\A  ProxyCommand ' })
      $want = '  ProxyCommand "' + ($expectProgram -replace '%', '%%') + '" ' + $aliasName
      if ($line.Count -ne 1 -or $line[0] -ine $want) { $p.Add('The alias''s ProxyCommand does not name agentspace-ssh-proxy.exe in the operator''s own folder, then the alias') }
    }
    Write-HpProblems "${sec}: With the program copied beside the operator the same command succeeds, and the alias's ProxyCommand names that copy" $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-alias-no-proxy-program.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Usage-error conformance for the ProxyCommand's argument handling (docs/operator.md, "The
# ProxyCommand"). The program under test is the one AGENTSPACE_SSH_PROXY_CMD names; unset, it takes
# its default from conformance/prototype/defaults.ps1 (docs/conformance.md, "Naming the program
# under test"). Every case ends before a tunnel starts, with exit 1 and stdout empty.
#
# Run as: pwsh -NoProfile -File conformance/operator/test-proxy-arguments.ps1
. (Join-Path $PSScriptRoot 'helpers.ps1')

$secondText = 'agentspace: ssh-proxy.cmd takes at most one argument, the SSH host alias.'
$aliasText = 'agentspace: -Alias MUST be an SSH host alias'
$section = 'operator.md#the-proxycommand'

# Each Arguments array is the argument list as the program receives it. `a;b` is one element that
# cmd.exe splits at the semicolon, which makes it two arguments, the second-argument case.
$cases = @(
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a/b extra refused before up"; Arguments = @('a/b', 'extra'); Text = $secondText }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a;b (two arguments) refused before up"; Arguments = @('a;b'); Text = $secondText }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd -Fx fails in up on the alias, starting no tunnel"; Arguments = @('-Fx'); Text = $aliasText }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd `"`" fails in up on the alias, starting no tunnel"; Arguments = @(''); Text = $aliasText }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd `"a b`" fails in up on the alias, starting no tunnel"; Arguments = @('a b'); Text = $aliasText }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a/b fails in up on the alias, starting no tunnel"; Arguments = @('a/b'); Text = $aliasText }
)

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status SKIPPED -Reason $gate.Reason }
  } elseif (-not $program) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program' }
  } else {
    foreach ($case in $cases) {
      # Exit 1 for both groups: the second-argument refusal, and a failed up, which ends the
      # ProxyCommand before its tunnel step (operator.md, step 4).
      $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $h -Arguments $case.Arguments -TimeoutSeconds 60
      Write-RefusalResult -Result $r -ExpectCode 1 -ExpectText $case.Text -Name $case.Name
    }
  }
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

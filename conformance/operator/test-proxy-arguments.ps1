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

# The refusal names the program's own file (operator.md, "The ProxyCommand", step 1; B-29), so it is
# matched with any name where the file's name goes, on a line of its own.
$secondPattern = '^agentspace: \S+ takes at most one argument, the SSH host alias\.\s*$'
$aliasText = 'agentspace: -Alias MUST be an SSH host alias'
$aliasPattern = '^' + [regex]::Escape($aliasText)
$section = 'operator.md#the-proxycommand'

# Each Arguments array is the argument list as the program receives it. `a;b` is one element that
# a .cmd program has cmd.exe split at the semicolon into two arguments, the second-argument case,
# and that a native program receives as one, which fails the alias check (operator.md, "The
# ProxyCommand", B-28). Either text is right, with exit 1 both ways.
$cases = @(
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a/b extra refused before up"; Arguments = @('a/b', 'extra'); Pattern = $secondPattern }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a;b (two arguments) refused before up"; Arguments = @('a;b'); Pattern = "($secondPattern)|($aliasPattern)" }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd -Fx fails in up on the alias, starting no tunnel"; Arguments = @('-Fx'); Pattern = $aliasPattern }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd `"`" fails in up on the alias, starting no tunnel"; Arguments = @(''); Pattern = $aliasPattern }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd `"a b`" fails in up on the alias, starting no tunnel"; Arguments = @('a b'); Pattern = $aliasPattern }
  [pscustomobject]@{ Name = "${section}: ssh-proxy.cmd a/b fails in up on the alias, starting no tunnel"; Arguments = @('a/b'); Pattern = $aliasPattern }
)

$h = New-ConformanceHome
try {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $trip = Get-OperatorTripwireEnvironment -ConformanceHome $h
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status SKIPPED -Reason $gate.Reason }
  } elseif (-not $program) {
    foreach ($case in $cases) { Write-ConformanceResult -Name $case.Name -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program' }
  } else {
    foreach ($case in $cases) {
      # Exit 1 for both groups: the second-argument refusal, and a failed up, which ends the
      # ProxyCommand before its tunnel step (operator.md, step 4).
      $r = Invoke-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD' -ConformanceHome $h -Arguments $case.Arguments -Environment $trip -TimeoutSeconds 60
      Write-RefusalResult -Result $r -ExpectCode 1 -ExpectPattern $case.Pattern -Name $case.Name
    }
  }
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

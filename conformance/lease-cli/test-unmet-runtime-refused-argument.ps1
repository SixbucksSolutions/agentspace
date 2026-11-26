#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0083 point 1; #0110 point 3), a MUST: every run that fails MUST still end with one
# error document on stderr and an exit code from the table, whatever stage it failed at, and the
# program MUST receive its arguments exactly as the caller's shell passed them, so an argument such as
# `-:x` reaches the parser as an argument.
#   - docs/lease-api.md, "The program" (arguments exactly as passed; every run that fails ends with
#     one error document), "Command lines" (an argument that starts with `-` is an option; anything
#     that is not `--` and a flag name is an unknown option; a single `-` before a value is that value)
#     and "Exit codes and errors" (the table; `usage` and `invalid_value` exit 2).
# Card: E0.06-T04-S03 (#0133), carrying #0110 point 3. What the prototype does instead: its runtime
# refuses `-:x` before the CLI starts, with exit 1 and no error document (#0083).
#
# Each line is run once. In an option position the argument is an unknown option, so `usage`, exit 2,
# and no `field` (a `usage` error carries one only in five cases, none of them this). In a flag's value
# position it is the value, so `invalid_value` for that flag, exit 2. Each failing run MUST put nothing
# on stdout and exactly one compact error document and one line feed on stderr, which validates against
# the error schema.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. No line issues a credential or finds a lease, so no CA is needed. It reads no
#   real folder, loads no prototype code and names no prototype path. It deletes and writes nothing. A
#   failure reason never quotes the program's output.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-runtime-refused-argument.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'lease-api.md#the-program'
$id = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'
$lab = $null
try {
  $lab = New-ConformanceHome
  $cases = @(
    # An unknown option, in each position it can be in.
    @{ Line = @('-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('-:'); Code = 'usage'; Field = '' }
    @{ Line = @('-:x=y'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'list', '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'get', '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'get', $id, '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'renew', '-:x', $id); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'release', $id, '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'create', '-:x'); Code = 'usage'; Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '-:x'); Code = 'usage'; Field = '' }
    # The same argument as a flag's value is the value, and the value's own check refuses it.
    @{ Line = @('lease', 'create', '--project', '-:x'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project=-:x'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '-:x'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size', '-:x'); Code = 'invalid_value'; Field = 'size' }
    @{ Line = @('lease', 'list', '--state', '-:x'); Code = 'invalid_value'; Field = 'state' }
  )
  foreach ($case in $cases) {
    $run = Invoke-AgentspaceCli -ConformanceHome $lab -Arguments $case.Line
    Complete-ConformanceCheck -Problems (Get-HardeningErrorProblems -Run $run -Code $case.Code -Field $case.Field) `
      -Name "$sec`: $(Format-ConformanceLine $case.Line) ends in one $($case.Code) error document, exit 2$(if ($case.Field) { ", field $($case.Field)" } else { ', no field' })"
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-runtime-refused-argument.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

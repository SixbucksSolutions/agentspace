#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check (D34): help at every level of the lease CLI, and --help in odd positions.
# It runs the program AGENTSPACE_CMD names, as a separate process, in a throwaway home, and compares
# its stdout with the help blocks that docs/lease-api.md gives, read at run time.
# It never reads or writes the real ~/.ssh or %USERPROFILE%\.agentspace, and loads no prototype code.
# Run: pwsh -NoProfile -File conformance/lease-cli/test-help.ps1
# Exits 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

$h = $null
try {
  $h = New-ConformanceHome
  $blocks = Get-ConformanceHelpBlocks
  $id = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'

  # The seven levels, each asked in both spellings (lease-api.md#command-lines: -h and --help).
  $levels = [ordered]@{
    'agentspace'               = @()
    'agentspace lease'         = @('lease')
    'agentspace lease create'  = @('lease', 'create')
    'agentspace lease get'     = @('lease', 'get')
    'agentspace lease renew'   = @('lease', 'renew')
    'agentspace lease release' = @('lease', 'release')
    'agentspace lease list'    = @('lease', 'list')
  }
  foreach ($level in $levels.Keys) {
    $anchor = $level -replace ' ', '-'
    foreach ($flag in '--help', '-h') {
      $line = @($levels[$level]) + $flag
      Test-ConformanceHelpRun -ConformanceHome $h -Level $level -Blocks $blocks -Arguments $line `
        -Name "lease-api.md#${anchor}: $(Format-ConformanceLine $line) prints the help block of the doc, exit 0"
    }
  }

  # --help in any position, and in any case (lease-api.md#command-lines, "Which error is reported", rule 1).
  $odd = @(
    @{ Line = @('lease', 'create', '--ttl', '--help'); Level = 'agentspace lease create'; Why = 'in a flag value position' }
    @{ Line = @('lease', 'create', '--ttl', '-H'); Level = 'agentspace lease create'; Why = 'as -H in a flag value position' }
    @{ Line = @('LEASE', 'CREATE', '-H'); Level = 'agentspace lease create'; Why = 'with the command and the flag in capitals' }
    @{ Line = @('lease', 'create', '--help', '--ttl', '99d'); Level = 'agentspace lease create'; Why = 'before a bad value, which is never checked' }
    @{ Line = @('lease', 'create', '--bogus', '--project', '-h'); Level = 'agentspace lease create'; Why = 'after an unknown option and a flag with no value' }
    @{ Line = @('lease', 'create', '--project', 'x', '--project', 'y', '--help'); Level = 'agentspace lease create'; Why = 'after a flag given twice' }
    @{ Line = @('lease', 'renew', $id, '--ttl', '-H'); Level = 'agentspace lease renew'; Why = 'as -H after the ID and --ttl' }
    @{ Line = @('lease', 'get', $id, $id, '--help'); Level = 'agentspace lease get'; Why = 'after an unexpected second ID' }
    @{ Line = @('lease', 'list', '--state', 'ready', '--state', '--HELP'); Level = 'agentspace lease list'; Why = 'as --HELP in a flag value position' }
    @{ Line = @('Lease', '--bogus', '--Help'); Level = 'agentspace lease'; Why = 'after an unknown option, at the lease level' }
    @{ Line = @('--help', 'lease'); Level = 'agentspace'; Why = 'before the command, which prints the top level' }
    @{ Line = @('--help', 'frob'); Level = 'agentspace'; Why = 'before an unknown command' }
    @{ Line = @('lease', '--help', 'frob'); Level = 'agentspace lease'; Why = 'before an unknown subcommand' }
    @{ Line = @('--bogus', '--help'); Level = 'agentspace'; Why = 'after an unknown option, at the top level' }
  )
  foreach ($case in $odd) {
    Test-ConformanceHelpRun -ConformanceHome $h -Level $case.Level -Blocks $blocks -Arguments $case.Line `
      -Name "lease-api.md#command-lines: $(Format-ConformanceLine $case.Line) prints the help of '$($case.Level)', $($case.Why)"
  }

  # --help= is a value, never a request for help (lease-api.md#command-lines: a flag's value).
  $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'x', '--ttl=--help')
  $p = Get-ConformanceErrorProblems -Run $r -Code invalid_value -Field ttl
  Complete-ConformanceCheck -Name "lease-api.md#command-lines: $(Format-ConformanceLine @('lease', 'create', '--project', 'x', '--ttl=--help')) takes --help as the value, so the TTL is refused" -Problems $p
  $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'x', '--ttl=-h')
  $p = Get-ConformanceErrorProblems -Run $r -Code invalid_value -Field ttl
  Complete-ConformanceCheck -Name "lease-api.md#command-lines: $(Format-ConformanceLine @('lease', 'create', '--project', 'x', '--ttl=-h')) takes -h as the value, so the TTL is refused" -Problems $p
} catch {
  Write-ConformanceResult -Name 'test-help.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(ConvertTo-ConformanceMasked $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h) { Remove-ConformanceHome -ConformanceHome $h }
}
Exit-ConformanceRun

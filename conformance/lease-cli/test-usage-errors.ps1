#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check (D34): the lease CLI's usage errors, the exit code and the `field` of each, and
# the order in which they are reported. It runs the program AGENTSPACE_CMD names, as a separate
# process, in a throwaway home. Each failing run MUST give the exit code of its error code, nothing on
# stdout, and one compact error document on stderr that validates against the error schema.
# `field` is checked here because the error schema cannot say which usage errors carry it
# (docs/lease-api.md#schemas): exactly six cases MUST, and every other usage error MUST NOT. The sixth, `--plan`
# with `--size` (field `plan`, contract v1), and the ranking of that error among the others are in
# test-plan-errors.ps1; the five of contract v0 are here.
# It never reads or writes the real ~/.ssh or %USERPROFILE%\.agentspace, and loads no prototype code.
# Run: pwsh -NoProfile -File conformance/lease-cli/test-usage-errors.ps1
# Exits 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

$h = $null
try {
  $h = New-ConformanceHome
  $id = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'

  # Each case: the line, and the field the usage error MUST carry ('' when it MUST NOT carry one).
  $cases = @(
    # No command or subcommand, and an unknown one (reading stops there, so a later --help is ignored).
    @{ Line = @(); Field = '' }
    @{ Line = @('lease'); Field = '' }
    @{ Line = @('frob'); Field = '' }
    @{ Line = @('frob', '--help'); Field = '' }
    @{ Line = @('lease', 'frob'); Field = '' }
    @{ Line = @('lease', 'frob', '--help'); Field = '' }
    @{ Line = @('--project', 'x', 'lease', 'create'); Field = '' }
    # An unknown option, at every level, and what counts as one.
    @{ Line = @('--bogus'); Field = '' }
    @{ Line = @('--help=yes'); Field = '' }
    @{ Line = @('lease', '--bogus'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '--bogus'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '-ttl', '8h'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '-p', 'y'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl:'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '-'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '--'); Field = '' }
    @{ Line = @('lease', 'create', '--project', 'x', '--help=yes'); Field = '' }
    @{ Line = @('lease', 'get', $id, '--ttl', '8h'); Field = '' }
    @{ Line = @('lease', 'get', '--ttl', '8h'); Field = '' }
    @{ Line = @('lease', 'release', $id, '--ttl', '8h'); Field = '' }
    @{ Line = @('lease', 'renew', $id, '--state', 'ready'); Field = '' }
    @{ Line = @('lease', 'list', '--bogus'); Field = '' }
    # An unexpected positional argument.
    @{ Line = @('lease', 'create', '--project', 'x', 'extra'); Field = '' }
    @{ Line = @('lease', 'get', $id, $id); Field = '' }
    @{ Line = @('lease', 'renew', $id, 'extra'); Field = '' }
    @{ Line = @('lease', 'release', $id, $id); Field = '' }
    @{ Line = @('lease', 'list', 'extra'); Field = '' }
    # A missing --project or lease ID: field project or id.
    @{ Line = @('lease', 'create'); Field = 'project' }
    @{ Line = @('lease', 'create', '--ttl', '8h'); Field = 'project' }
    @{ Line = @('lease', 'get'); Field = 'id' }
    @{ Line = @('lease', 'renew'); Field = 'id' }
    @{ Line = @('lease', 'renew', '--ttl', '8h'); Field = 'id' }
    @{ Line = @('lease', 'release'); Field = 'id' }
    # A flag with no value: it is last, or the next argument starts with --. The field names the flag.
    @{ Line = @('lease', 'create', '--project'); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', '--size', 'm'); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl'); Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '--size', 'm'); Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--platform'); Field = 'platform' }
    @{ Line = @('lease', 'create', '--project', 'x', '--profile'); Field = 'profile' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size'); Field = 'size' }
    @{ Line = @('lease', 'renew', $id, '--ttl'); Field = 'ttl' }
    @{ Line = @('lease', 'list', '--project'); Field = 'project' }
    @{ Line = @('lease', 'list', '--state'); Field = 'state' }
    @{ Line = @('lease', 'list', '--state', 'ready', '--state'); Field = 'state' }
    # A flag with an empty value, in both forms. A value of only whitespace is not empty (test-values.ps1).
    @{ Line = @('lease', 'create', '--project='); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', ''); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl='); Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', ''); Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--platform='); Field = 'platform' }
    @{ Line = @('lease', 'create', '--project', 'x', '--profile', ''); Field = 'profile' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size='); Field = 'size' }
    @{ Line = @('lease', 'renew', $id, '--ttl='); Field = 'ttl' }
    @{ Line = @('lease', 'list', '--project='); Field = 'project' }
    @{ Line = @('lease', 'list', '--state', ''); Field = 'state' }
    @{ Line = @('lease', 'list', '--state='); Field = 'state' }
    # A flag given twice, in any case. Only list --state MAY repeat.
    @{ Line = @('lease', 'create', '--project', 'x', '--project', 'y'); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--Project=y'); Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8h', '--TTL', '9h'); Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size', 'm', '--size', 'm'); Field = 'size' }
    @{ Line = @('lease', 'renew', $id, '--ttl', '8h', '--ttl', '9h'); Field = 'ttl' }
    @{ Line = @('lease', 'list', '--project', 'a', '--project', 'b'); Field = 'project' }
    @{ Line = @('lease', 'list', '--project', 'a', '--PROJECT=a'); Field = 'project' }
    # Which error is reported (lease-api.md#command-lines): the first in line order, and a usage error
    # before any bad value, wherever each is.
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8', '--bogus'); Field = '' }
    @{ Line = @('lease', 'create', '--ttl', '8', '--project'); Field = 'project' }
    @{ Line = @('lease', 'create', '--bogus', '--ttl'); Field = '' }
    @{ Line = @('lease', 'create', '--ttl', '--bogus'); Field = 'ttl' }
    @{ Line = @('lease', 'get', 'not-a-uuid', '--ttl', '8h'); Field = '' }
    @{ Line = @('lease', 'get', 'not-a-uuid', 'extra'); Field = '' }
    @{ Line = @('lease', 'list', '--project', '-x-', '--bogus'); Field = '' }
  )
  foreach ($case in $cases) {
    $where = if ($case.Field) { "field '$($case.Field)'" } else { 'no field' }
    Test-ConformanceErrorRun -ConformanceHome $h -Arguments $case.Line -Code usage -Field $case.Field `
      -Name "lease-api.md#command-lines: $(Format-ConformanceLine $case.Line) is a usage error with $where, exit 2"
  }

  # Commands and subcommands are matched without regard to case (lease-api.md#command-lines).
  Test-ConformanceErrorRun -ConformanceHome $h -Arguments @('LEASE', 'FROB') -Code usage `
    -Name "lease-api.md#command-lines: $(Format-ConformanceLine @('LEASE', 'FROB')) is a usage error for the subcommand, exit 2"
  Test-ConformanceErrorRun -ConformanceHome $h -Arguments @('Lease', 'Get') -Code usage -Field id `
    -Name "lease-api.md#command-lines: $(Format-ConformanceLine @('Lease', 'Get')) is read as lease get, so the ID is missing, exit 2"
} catch {
  Write-ConformanceResult -Name 'test-usage-errors.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(ConvertTo-ConformanceMasked $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h) { Remove-ConformanceHome -ConformanceHome $h }
}
Exit-ConformanceRun

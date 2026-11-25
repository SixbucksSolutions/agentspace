#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check (D34): the lease CLI's `invalid_value` and `out_of_range` errors, the `field` of
# each, the order in which bad values are reported, and the error schema's refusal of a line feed.
# It runs the program AGENTSPACE_CMD names, as a separate process, in a throwaway home. Every case
# fails before any lease is looked up or any credential is issued, so no CA is needed.
# It never reads or writes the real ~/.ssh or %USERPROFILE%\.agentspace, and loads no prototype code.
# Run: pwsh -NoProfile -File conformance/lease-cli/test-values.ps1
# Exits 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

$h = $null
try {
  $h = New-ConformanceHome
  $id = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'

  # Each case: the line, the error code, and the field.
  $cases = @(
    # A value outside its accepted forms is invalid_value (lease-api.md#values).
    @{ Line = @('lease', 'create', '--project', 'a_b'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', ' '); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', "`t"); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', '-x-'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', ('a' * 33)); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', "a`tb"); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--project', 'x', '--platform', 'linux'); Code = 'invalid_value'; Field = 'platform' }
    @{ Line = @('lease', 'create', '--project', 'x', '--platform', 'linux/arm64/v8/x'); Code = 'invalid_value'; Field = 'platform' }
    @{ Line = @('lease', 'create', '--project', 'x', '--profile', 'agent-linux@0'); Code = 'invalid_value'; Field = 'profile' }
    @{ Line = @('lease', 'create', '--project', 'x', '--profile', 'agent linux'); Code = 'invalid_value'; Field = 'profile' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size', 'toolarge9'); Code = 'invalid_value'; Field = 'size' }
    @{ Line = @('lease', 'create', '--project', 'x', '--size', ' '); Code = 'invalid_value'; Field = 'size' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '28800'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '0'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8M'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8 M'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '1.5h'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '1w'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', 'P1M'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '-5m'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '1h1h'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '30m1h'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', 'PT'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', ' '); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl=1:30'); Code = 'invalid_value'; Field = 'ttl' }
    # A well-formed value outside its range is out_of_range, never clamped, and never internal.
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '48h'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '24h1s'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '1441m'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '59s'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '0m'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', 'PT0S'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', 'P2D'); Code = 'out_of_range'; Field = 'ttl' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl=99999999999999999999h'); Code = 'out_of_range'; Field = 'ttl' }
    # The first bad value in the order of the table (project, platform, profile, plan, size, ttl) is reported. A line
    # with no --plan is as in contract v0; the plan's own place in that order is in test-plan-errors.ps1.
    @{ Line = @('lease', 'create', '--project', 'bad_', '--ttl', '8'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'create', '--ttl', '8', '--project', 'x', '--size', 'toolarge9'); Code = 'invalid_value'; Field = 'size' }
    @{ Line = @('lease', 'create', '--size', 'toolarge9', '--platform', 'linux', '--project', 'x'); Code = 'invalid_value'; Field = 'platform' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '48h', '--profile', 'agent-linux@0'); Code = 'invalid_value'; Field = 'profile' }
    @{ Line = @('lease', 'create', '--project', 'x', '--ttl', '8', '--platform', 'linux/arm64'); Code = 'invalid_value'; Field = 'ttl' }
    # get, renew and release: the ID is checked before anything else, and before any lookup.
    @{ Line = @('lease', 'get', 'not-a-uuid'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', ''); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', '3f2a9c1e-7b4d-1e6a-8c2f-0a1b2c3d4e5f'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', '3f2a9c1e-7b4d-4e6a-cc2f-0a1b2c3d4e5f'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', "urn:uuid:$id"); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', '3f2a9c1e7b4d4e6a8c2f0a1b2c3d4e5f'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', "$id/x"); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'get', '..\x'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'release', "{$id}"); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'renew', 'nope', '--ttl', '8'); Code = 'invalid_value'; Field = 'id' }
    @{ Line = @('lease', 'renew', $id, '--ttl', '8'); Code = 'invalid_value'; Field = 'ttl' }
    @{ Line = @('lease', 'renew', $id, '--ttl', 'P2D'); Code = 'out_of_range'; Field = 'ttl' }
    # list: a bad project, or else a bad state name, which the message quotes as '' when it is empty.
    @{ Line = @('lease', 'list', '--state', 'ready,bogus'); Code = 'invalid_value'; Field = 'state' }
    @{ Line = @('lease', 'list', '--state', 'ready,'); Code = 'invalid_value'; Field = 'state' }
    @{ Line = @('lease', 'list', '--state', 'ready,,pending'); Code = 'invalid_value'; Field = 'state' }
    @{ Line = @('lease', 'list', '--state', ' '); Code = 'invalid_value'; Field = 'state' }
    @{ Line = @('lease', 'list', '--state', 'ready', '--state', 'bogus'); Code = 'invalid_value'; Field = 'state' }
    @{ Line = @('lease', 'list', '--project', '-x-'); Code = 'invalid_value'; Field = 'project' }
    @{ Line = @('lease', 'list', '--state', 'bogus', '--project', '-x-'); Code = 'invalid_value'; Field = 'project' }
  )
  foreach ($case in $cases) {
    $anchor = if ($case.Line[1] -eq 'create' -and $case.Field -eq 'ttl') { 'ttl' } elseif ($case.Field -eq 'ttl') { 'exit-codes-and-errors' } else { 'values' }
    Test-ConformanceErrorRun -ConformanceHome $h -Arguments $case.Line -Code $case.Code -Field $case.Field `
      -Name "lease-api.md#${anchor}: $(Format-ConformanceLine $case.Line) is $($case.Code) for '$($case.Field)', exit 2"
  }

  # A message holds no control character, and the schema refuses one. The check is proved able to fail:
  # the program's own error validates, and the same error with a line feed in its message does not.
  $r = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', 'not-a-uuid')
  $p = Get-ConformanceErrorProblems -Run $r -Code invalid_value -Field id
  if (Test-ConformanceSchema -Json '{"code":"usage","message":"a\nb"}' -Schema 'error.output.schema.json') { $p.Add('The error schema accepted a message with a line feed') }
  Complete-ConformanceCheck -Name 'lease-api.md#schemas: the error schema refuses a message with a line feed, and the real error of lease get not-a-uuid validates' -Problems $p
} catch {
  Write-ConformanceResult -Name 'test-values.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h) { Remove-ConformanceHome -ConformanceHome $h }
}
Exit-ConformanceRun

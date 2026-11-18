#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype, when it is run from a checkout that holds no .instance-id of
# its own, such as a git worktree. A rewrite MUST pass it. This is not a regression when it fails
# against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement, a MUST: `create` MUST NOT read, check for or depend on the operator's instance-ID
# cache. With AGENTSPACE_PROXY_COMMAND unset, and no cache anywhere in the home, a ready `create` MUST
# succeed; a missing or stale cache fails the connection, never the `create`.
#   - docs/state.md, "The alias file" (the bullet that starts "`create` MUST NOT read, check for or
#     need the instance-ID cache").
#   - docs/driver.md, "How it is reached".
#   - D21 (the architect's ruling of 2026-11-18, from the clean-room review, #0080).
# What the prototype does instead: with no override, it refuses to write the alias when the checkout
# its code runs from holds no `.instance-id`, its own copy of the cache, and so `create` fails with
# `internal`. From the main checkout on the laptop, which holds one, the prototype passes this file.
# A check cannot hide that file and MUST NOT touch it, so that run proves nothing about the prototype
# (docs/conformance.md names this exception).
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - It writes nothing itself, and seeds no instance-ID cache: the throwaway home has none, and it
#   checks that before the `create`.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-default-transport-no-cache.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

function Add-UnmetResult {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}

$alias = 'state.md#the-alias-file'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$alias`: A ready create under the default transport succeeds with no instance-ID cache" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    # What the failing line assumes: the home holds no instance-ID cache (docs/state.md, "Files in the
    # checkout"), so the create below has none to read.
    $cache = Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $lab.UserProfile '.agentspace\instance-id')
    $p = [System.Collections.Generic.List[string]]::new()
    if (Test-Path -LiteralPath $cache) { $p.Add('The throwaway home already holds an instance-ID cache, so this file would show nothing') }
    Add-UnmetResult -Name "state.md#files-in-the-checkout: The throwaway home holds no instance-ID cache before the create" -Problems $p

    # The MUST: with AGENTSPACE_PROXY_COMMAND removed, the program forms its default line, and the
    # create succeeds without the cache.
    $r = New-CcReadyLease -Lab $lab -Project 'nocache' -EnvVars @{ AGENTSPACE_PROXY_COMMAND = $null }
    $p = [System.Collections.Generic.List[string]]::new()
    if (-not $r.Ok) {
      $code = if ($r.Run.ErrorJson -is [hashtable]) { [string]$r.Run.ErrorJson['code'] } else { 'none' }
      $p.Add("The create did not print a ready lease: exit $($r.Run.ExitCode), error code '$code'")
    }
    Add-UnmetResult -Name "$alias`: With AGENTSPACE_PROXY_COMMAND unset and no instance-ID cache, a ready create MUST succeed (driver.md#how-it-is-reached; D21)" -Problems $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-default-transport-no-cache.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 4): the lease CLI never
# reads or needs the instance-ID cache. It enforces:
#   - docs/state.md, "The alias file": `create` MUST NOT read, check for or need the instance-ID
#     cache, with or without the override (`AGENTSPACE_PROXY_COMMAND`).
#   - docs/state.md, "Files in the checkout": the cache is `%USERPROFILE%\.agentspace\instance-id.json`,
#     and the lease CLI MUST NOT read or need it. docs/driver.md, "How it is reached".
#   - docs/state.md, "Requirements the prototype does not meet yet" (the instance-ID cache's place;
#     `create` never reads or needs the cache).
# A cache that cannot be read (a folder where the file is, and a file that is not JSON) fails an
# operator command (operator.md, "Laptop files"), so a lease command that failed or changed on seeing
# one would be reading it. Each variant is seeded at the cache's place between commands, and `create`,
# `get`, `renew` and `release` MUST succeed as they do with no cache. Each is run with the check's
# ProxyCommand override and, when a `create` under the default transport works with no cache at all,
# under the default transport too (a seeded cache that a build reads would then change the outcome).
#
# - NOT SHOWN by this file: that the operator and the ProxyCommand READ the cache at that place. They
#   are the operator's, and conformance/operator/test-unmet-proxy-steps.ps1 and test-unmet-push.ps1
#   seed it there and require the cached instance to be the one used.
# - Every write and delete calls Assert-ConformancePath at the call, with no program run between the
#   call and the write. A lease ID or alias read from the program's output names a file only after the
#   document passed its schema (Get-CcLeaseDir, Get-CcAliasPath). A failure reason never quotes the
#   program's output. The cache's made-up ID is not an instance ID of any account.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-ignores-instance-cache.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#the-alias-file'
$default = @{ AGENTSPACE_PROXY_COMMAND = $null }
$fakeId = 'i-1234567890abcdef0'

# Puts the cache variant at the cache's place, or removes it (-Kind none).
function Set-CacheVariant($Lab, [string]$Kind) {
  $path = Join-Path $Lab.UserProfile '.agentspace\instance-id.json'
  if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $Lab -Path $path) -Recurse -Force }
  switch ($Kind) {
    'folder' { New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $Lab -Path $path) | Out-Null }
    'garbage' { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $Lab -Path $path), 'this is not json') }
    'valid' { [IO.File]::WriteAllText((Assert-ConformancePath -ConformanceHome $Lab -Path $path), ('{"version":0,"instance_id":"' + $fakeId + '"}')) }
  }
}

$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: With a cache that cannot be read, the lease CLI's commands succeed" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    # Whether a create under the default transport works with no cache at all. When it does not, that
    # is test-unmet-default-transport-no-cache.ps1's failure, and the default-transport runs are skipped.
    Set-CacheVariant $lab 'none'
    $base = New-CcReadyLease -Lab $lab -Project 'cache-base' -EnvVars $default
    $defaultWorks = $base.Ok
    # The skip is for one failure only: the valid `internal` error that test-unmet-default-transport-no-cache.ps1
    # holds a build to. Any other failure of this create is a FAIL of this file, below.
    $baseProblems = if ($defaultWorks) { $null } else { Get-HardeningErrorProblems -Run $base.Run -Code 'internal' }
    $defaultInternal = (-not $defaultWorks) -and $baseProblems.Count -eq 0
    foreach ($kind in 'folder', 'garbage', 'valid') {
      $what = switch ($kind) { 'folder' { 'a folder where the cache is' } 'garbage' { 'a cache that is not JSON' } 'valid' { 'a valid cache' } }
      Set-CacheVariant $lab $kind
      $l = New-CcReadyLease -Lab $lab -Project "cache-$kind"
      if (-not $l.Ok) {
        Write-ConformanceResult -Name "$sec`: With $what, a ready create succeeds" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates"
        continue
      }
      $id = [string]$l.Doc['id']
      Write-ConformanceResult -Name "$sec`: With $what, a ready create succeeds" -Status PASS
      $ops = @(
        @{ Sub = 'get'; State = 'ready' }
        @{ Sub = 'renew'; State = 'ready' }
        @{ Sub = 'release'; State = 'released' }
      )
      foreach ($o in $ops) {
        $run = Invoke-CcLease -Lab $lab -Arguments @($o.Sub, $id)
        $ok = (Test-CcSuccess $run) -and $run.Json['id'] -ceq $id -and $run.Json['state'] -ceq $o.State
        Complete-ConformanceCheck -Name "$sec`: With $what, $($o.Sub) succeeds and prints the lease $($o.State)" -Problems $(if ($ok) { $null } else { [System.Collections.Generic.List[string]]@("$($o.Sub) did not print the lease $($o.State)") })
      }
      if ($defaultWorks) {
        $d = New-CcReadyLease -Lab $lab -Project "cache-d-$kind" -EnvVars $default
        Complete-ConformanceCheck -Name "$sec`: With $what, a ready create under the default transport succeeds, as it does with no cache" -Problems $(if ($d.Ok) { $null } else { [System.Collections.Generic.List[string]]@('create did not print a ready lease that validates') })
      }
    }
    $defaultName = "$sec`: With each cache variant, a ready create under the default transport succeeds, as it does with no cache"
    if ($defaultInternal) {
      Write-ConformanceResult -Name $defaultName -Status SKIPPED -Reason 'A create under the default transport fails with a valid internal error with no cache at all, which is test-unmet-default-transport-no-cache.ps1''s failure, so the cache cannot be what is tested'
    } elseif (-not $defaultWorks) {
      Write-ConformanceResult -Name $defaultName -Status FAIL -Reason "A create under the default transport with no cache at all neither succeeded nor failed with a valid internal error: $(ConvertTo-ConformanceMasked ($baseProblems -join '; '))"
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-lease-ignores-instance-cache.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

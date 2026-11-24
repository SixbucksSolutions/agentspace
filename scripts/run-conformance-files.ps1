# Runs conformance files from a review checkout against a build's output folder, one at a time
# (docs/architect.md, "Reviewing a pull request", steps 3 and 4). Process tool, not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage: pwsh -NoProfile -File scripts\run-conformance-files.ps1 -Checkout <folder> -Out <folder>
#          -Files 'conformance\keeper\test-keeper.ps1,conformance\schemas\test-schemas.ps1' [-Dist <folder>]
#   -Checkout  The checkout whose conformance files run, such as a review worktree under
#              .claude\worktrees\. A worktree under the scratchpad's long path fails git's checkout of
#              box/tests' fixtures ("Filename too long").
#   -Dist      The build's output folder; the checkout's own target\dist by default, where
#              `cargo xtask dist` writes it. Another build MAY be named when no crate differs.
#   -Files     A comma-separated list, as one argument: `pwsh -File` passes an array as one string.
#   -Out       A folder for each file's whole output, one log per file.
#
# It sets the four AGENTSPACE_*_CMD variables to the build's programs, and stops before any file runs
# unless each names a file that exists. It prints one line per file (its exit code, its seconds and its
# summary line), then the files that exited non-zero, and itself exits 1 when any did and 0 when none
# did (v0.09-T04: it exited 0 whatever the files did). A run beside other suites MAY fail on timing: a
# timing failure is run once more alone before it is called a finding, and both runs are reported.
param(
  [Parameter(Mandatory)][string]$Checkout,
  [Parameter(Mandatory)][string[]]$Files,
  [Parameter(Mandatory)][string]$Out,
  [string]$Dist
)
$ErrorActionPreference = 'Stop'
$Files = @($Files | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$dist = if ($Dist) { $Dist } else { Join-Path $Checkout 'target\dist' }
$env:AGENTSPACE_CMD = Join-Path $dist 'agentspace.exe'
$env:AGENTSPACE_OPERATOR_CMD = Join-Path $dist 'agentspace-operator.exe'
$env:AGENTSPACE_SSH_PROXY_CMD = Join-Path $dist 'agentspace-ssh-proxy.exe'
$env:AGENTSPACE_KEEPER_CMD = Join-Path $dist 'agentspace-keeper.exe'
foreach ($v in 'AGENTSPACE_CMD', 'AGENTSPACE_OPERATOR_CMD', 'AGENTSPACE_SSH_PROXY_CMD', 'AGENTSPACE_KEEPER_CMD') {
  if (-not (Test-Path -LiteralPath ([Environment]::GetEnvironmentVariable($v)) -PathType Leaf)) {
    throw "$v names a file that does not exist."
  }
}
'Every AGENTSPACE_*_CMD names a file that exists.'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Set-Location $Checkout
$failed = @()
foreach ($f in $Files) {
  $start = Get-Date
  $log = Join-Path $Out (($f -replace '[\\/]', '_') + '.log')
  & pwsh -NoProfile -File $f *> $log
  $code = $LASTEXITCODE
  $summary = Get-Content $log | Where-Object { $_ -match '^\d+ passed, \d+ failed, \d+ skipped$' } | Select-Object -Last 1
  "$f exit $code in $([int]((Get-Date) - $start).TotalSeconds)s: $summary"
  if ($code -ne 0) { $failed += $f }
}
if ($failed.Count -gt 0) {
  "Failed: $($failed -join ', ')"
  exit 1
}
'Failed: none'
exit 0

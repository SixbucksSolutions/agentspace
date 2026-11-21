#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement, a MUST: when agentspace's `Include` line is in the OpenSSH client configuration but
# below a `Host` or `Match` line, `create` MUST fail, change nothing, and say that a person has to
# move the line to the top of the file.
#   - docs/state.md, "The `Include` line" (the bullet that starts "If the line is not found above the
#     first block, but appears below it").
#   - docs/lease-api.md, "Messages" (the `internal` row, "Alias cannot be set up"). Its wording is a
#     template an implementation SHOULD follow and MAY word differently, so this file does not match
#     the template. It checks only what state.md makes a MUST: the message says to move the line, to
#     the top of the file. It does not require the word `Include`, and it does not check the wording.
# Card: E0.04-T03-S03 (#0086). What the prototype does instead: `create` fails with `internal` and
# changes nothing, as it MUST, but its message does not say to move anything to the top.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
# - Its only write is the throwaway home's OpenSSH client configuration, through
#   Assert-ConformancePath. It deletes nothing itself.
# - A failure reason never quotes the program's output, which can hold a path or a name.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-include-message.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')

$script:UnmetProxy = 'agentspace-conformance-proxy --not-run'

function Invoke-UnmetLease {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][string[]]$Arguments)
  Invoke-AgentspaceCli -ConformanceHome $Lab -Arguments (@('lease') + $Arguments) -Environment @{ AGENTSPACE_PROXY_COMMAND = $script:UnmetProxy }
}

function Add-UnmetResult {
  param([Parameter(Mandatory)][string]$Name, [AllowEmptyCollection()][System.Collections.Generic.List[string]]$Problems)
  if ($null -eq $Problems -or $Problems.Count -eq 0) { Write-ConformanceResult -Name $Name -Status PASS }
  else { Write-ConformanceResult -Name $Name -Status FAIL -Reason ($Problems -join '; ') }
}

# The problems with a run that was meant to fail with $Code: its exit code, nothing on stdout, and one
# error document on stderr that validates and carries the code. It never quotes stderr.
function Get-UnmetErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [Parameter(Mandatory)][int]$Exit)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -eq 0) { $p.Add("Exit 0, not $Exit`: the command succeeded"); return , $p }
  if ($Run.ExitCode -ne $Exit) { $p.Add("Exit $($Run.ExitCode), not $Exit") }
  if ($Run.StdoutBytes.Length -ne 0) { $p.Add("Stdout holds $($Run.StdoutBytes.Length) bytes") }
  $doc = $Run.ErrorJson
  if ($doc -isnot [hashtable]) { $p.Add("Stderr ($($Run.StderrBytes.Length) bytes) is not a JSON document"); return , $p }
  if (-not (Test-ConformanceSchema -Json $Run.Stderr.TrimEnd("`n") -Schema 'error.output.schema.json')) { $p.Add('The error document does not validate against error.output.schema.json') }
  if ($doc['code'] -cne $Code) { $p.Add("The error code is '$($doc['code'])', not '$Code'") }
  return , $p
}

function Get-UnmetFileNames {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return @() }
  return @(Get-ChildItem -LiteralPath $Path -Force | ForEach-Object Name | Sort-Object)
}

function Get-UnmetRecordCount {
  param([Parameter(Mandatory)]$Lab)
  $path = Join-Path $Lab.StateDir 'leases.json'
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return 0 }
  try { return @(([IO.File]::ReadAllText($path) | ConvertFrom-Json -Depth 50 -ErrorAction Stop).leases).Count } catch { return -1 }
}

$include = 'state.md#the-include-line'
$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$include`: An Include line below a Host line makes create say to move it to the top" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    # The line is there, but below the first Host line, so it belongs to that block.
    $original = [Text.UTF8Encoding]::new($false).GetBytes("Host elsewhere`n  User someone`nInclude agentspace.d/*`n")
    $config = Join-Path $lab.SshDir 'config'
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $config), $original)

    # Under D54 a create is ready only for a project with a ready host (E0.07-T04-S01).
    [void](Add-ConformanceReadyHost -ConformanceHome $lab -Project 'misplaced')
    $run = Invoke-UnmetLease $lab @('create', '--project', 'misplaced')
    $p = Get-UnmetErrorProblems -Run $run -Code 'internal' -Exit 1
    Add-UnmetResult -Name "$include`: An Include line below a Host line makes create fail with internal, exit 1, one error document and no stdout" -Problems $p

    $p = [System.Collections.Generic.List[string]]::new()
    $now = [IO.File]::ReadAllBytes($config)
    if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$now, [byte[]]$original)) { $p.Add('The config changed') }
    $extra = @(Get-UnmetFileNames $lab.SshDir | Where-Object { $_ -like 'config.agentspace-*' })
    if ($extra.Count -gt 0) { $p.Add("There are $($extra.Count) backup or temporary files beside the config") }
    $aliases = Get-UnmetFileNames (Join-Path $lab.SshDir 'agentspace.d')
    if ($aliases.Count -gt 0) { $p.Add("There are $($aliases.Count) alias files") }
    $leases = Get-UnmetFileNames (Join-Path $lab.UserProfile '.agentspace\leases')
    if ($leases.Count -gt 0) { $p.Add("There are $($leases.Count) entries under the leases folder") }
    $n = Get-UnmetRecordCount $lab
    if ($n -ne 0) { $p.Add($(if ($n -lt 0) { 'The state file is not a JSON document' } else { "The state file holds $n records" })) }
    Add-UnmetResult -Name "$include`: create MUST change nothing when the line is below a Host line" -Problems $p

    # What state.md makes a MUST of the message: it says a person has to move the line to the top of the file.
    $p = [System.Collections.Generic.List[string]]::new()
    $doc = $run.ErrorJson
    if ($doc -isnot [hashtable] -or $doc['message'] -isnot [string]) { $p.Add('There is no error message to read') }
    else {
      $message = [string]$doc['message']
      if ($message -inotmatch '\bmov(e|ed|ing)\b') { $p.Add('The message does not say to move anything') }
      if ($message -inotmatch '\btop\b') { $p.Add('The message does not say the top of the file') }
    }
    Add-UnmetResult -Name "$include`: create MUST say that a person has to move the line to the top of the file (state.md, the Include line; lease-api.md#messages)" -Problems $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-include-message.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

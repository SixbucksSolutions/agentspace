#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0083), a MUST: the program MUST receive its arguments exactly as the caller's
# shell passed them, and for any value but one that starts with `--` or is `-h`, a flag's two forms,
# `--project=<value>` and `--project <value>`, MUST give the same result, a value that holds a line
# feed included. A project name with a line feed inside it is `invalid_value` for `project`, exit 2.
#   - docs/lease-api.md, "The program" (arguments exactly as passed), "Command lines" (A flag's
#     value), "Values" (Project) and "Exit codes and errors".
# Card: E0.04-T03-S03 (#0086), carrying #0083. What the prototype does instead: its
# `bin\agentspace.cmd` is a cmd.exe batch file, and cmd.exe cuts the command line at the first line
# feed, so the CLI sees `a` for `--project "a<LF>b"` and makes a lease. The same cut turns the `=`
# form into a different command line again.
#
# Each run gives `--size xxl` before `--project`, a size no profile has, so that a program that cuts the
# line still reaches an unsatisfiable lease with no credential and needs no CA. (A program that cuts
# the line after `--project` loses the size, and would issue a credential.) Each run MUST give
# `invalid_value` for `project` and exit 2, and MUST make no record, lease folder or alias file.
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. The harness passes each argument through ProcessStartInfo.ArgumentList.
#   It reads no real folder, loads no prototype code and names no prototype path.
# - It deletes and writes nothing. A failure reason never quotes the program's output.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-argument-line-feed.ps1
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

# The problems with a run that was meant to fail with $Code on $Field: its exit code, nothing on
# stdout, and one error document on stderr that validates and carries the code and the field. It
# never quotes a stream.
function Get-UnmetErrorProblems {
  param([Parameter(Mandatory)]$Run, [Parameter(Mandatory)][string]$Code, [Parameter(Mandatory)][int]$Exit, [Parameter(Mandatory)][string]$Field)
  $p = [System.Collections.Generic.List[string]]::new()
  if ($Run.ExitCode -eq 0) { $p.Add("Exit 0, not $Exit`: the command succeeded"); return , $p }
  if ($Run.ExitCode -ne $Exit) { $p.Add("Exit $($Run.ExitCode), not $Exit") }
  if ($Run.StdoutBytes.Length -ne 0) { $p.Add("Stdout holds $($Run.StdoutBytes.Length) bytes") }
  $doc = $Run.ErrorJson
  if ($doc -isnot [hashtable]) { $p.Add("Stderr ($($Run.StderrBytes.Length) bytes) is not a JSON document"); return , $p }
  if (-not (Test-ConformanceSchema -Json $Run.Stderr.TrimEnd("`n") -Schema 'error.output.schema.json')) { $p.Add('The error document does not validate against error.output.schema.json') }
  if ($doc['code'] -cne $Code) { $p.Add("The error code is '$($doc['code'])', not '$Code'") }
  if (-not $doc.ContainsKey('field')) { $p.Add("There is no field, and '$Field' is wanted") }
  elseif ($doc['field'] -cne $Field) { $p.Add("The field is '$($doc['field'])', not '$Field'") }
  return , $p
}

function Get-UnmetFileNames {
  param([Parameter(Mandatory)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return @() }
  return @(Get-ChildItem -LiteralPath $Path -Force | ForEach-Object Name | Sort-Object)
}

# The records in the state file: 0 when there is no file, -1 when the file is not a JSON document.
function Get-UnmetRecordCount {
  param([Parameter(Mandatory)]$Lab)
  $state = Join-Path $Lab.StateDir 'leases.json'
  if (-not (Test-Path -LiteralPath $state -PathType Leaf)) { return 0 }
  try { return @(([IO.File]::ReadAllText($state) | ConvertFrom-Json -Depth 50 -ErrorAction Stop).leases).Count } catch { return -1 }
}

# What a run left behind: records beyond $RecordsBefore, entries under the leases folder, alias files.
function Get-UnmetLeftovers {
  param([Parameter(Mandatory)]$Lab, [Parameter(Mandatory)][int]$RecordsBefore)
  $found = [System.Collections.Generic.List[string]]::new()
  $n = Get-UnmetRecordCount $Lab
  if ($n -lt 0) { $found.Add('The state file is not a JSON document') }
  elseif ($n -gt $RecordsBefore) { $found.Add("The state file holds $($n - $RecordsBefore) new records") }
  $folders = Get-UnmetFileNames (Join-Path $Lab.UserProfile '.agentspace\leases')
  if ($folders.Count -gt 0) { $found.Add("There are $($folders.Count) entries under the leases folder") }
  $aliases = Get-UnmetFileNames (Join-Path $Lab.SshDir 'agentspace.d')
  if ($aliases.Count -gt 0) { $found.Add("There are $($aliases.Count) alias files") }
  return , $found
}

$cases = @(
  @{ Form = 'separate'; Label = '--project "a<LF>b" (the value is the next argument)'; Arguments = @('create', '--size', 'xxl', '--project', "a`nb") }
  @{ Form = 'equals'; Label = '--project=a<LF>b (the value follows an equals sign)'; Arguments = @('create', '--size', 'xxl', "--project=a`nb") }
)
$lab = $null
try {
  # A check that needs no credential needs no CA, so a home without one is still a home to run in.
  $lab = New-ConformanceHome
  foreach ($case in $cases) {
    $base = "lease-api.md#command-lines: create $($case.Label)"
    $recordsBefore = [Math]::Max(0, (Get-UnmetRecordCount $lab))
    $run = Invoke-UnmetLease $lab $case.Arguments
    $p = Get-UnmetErrorProblems -Run $run -Code 'invalid_value' -Exit 2 -Field 'project'
    Add-UnmetResult -Name "$base is invalid_value for project, exit 2, one error document and no stdout (#0083)" -Problems $p

    $left = Get-UnmetLeftovers -Lab $lab -RecordsBefore $recordsBefore
    Add-UnmetResult -Name "$base makes no record, no lease folder and no alias file (lease-api.md#concurrency-and-atomicity)" -Problems $left
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-argument-line-feed.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

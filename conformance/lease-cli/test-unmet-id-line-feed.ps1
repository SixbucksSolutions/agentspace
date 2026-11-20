#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirements, MUSTs: the program MUST receive its arguments exactly as the caller's shell passed
# them, a line feed included, and a lease ID is matched as a whole value, after its surrounding
# whitespace is trimmed, a line feed included (#0029 point 2; #0110 point 3).
#   - docs/lease-api.md, "The program" (arguments exactly as passed), "Input tolerance" (surrounding
#     whitespace, and every check matches the whole value) and "Lease ID".
#   - docs/lease-api.md, "Exit codes and errors": a quoted value's control characters are written as
#     `\u` and four lowercase hex digits, or the value is left out.
# Card: E0.06-T04-S03 (#0133), carrying #0110 point 3. What the prototype does instead: its
# `bin\agentspace.cmd` is a cmd.exe batch file, and cmd.exe cuts the command line at the first line
# feed, so the CLI never sees a line feed. A line feed INSIDE an ID is cut to the part before it, so
# `get <id><LF>x` finds the lease where the contract says `invalid_value`; and a line feed BEFORE an
# ID empties the argument, so a value the contract trims and accepts is refused.
#
# - Inside the value (`<id><LF>x`, `<id><LF>.`, two IDs on two lines): not surrounding whitespace, so
#   `invalid_value` for `id`, exit 2, for get, renew and release. The ID in these is well-formed and
#   names no lease, so a program that cuts it at the line feed answers `not_found` (exit 3) and
#   changes nothing, and no lease is ever released or renewed by this file. One `get` is also run on a
#   real lease, and MUST NOT print it.
# - Before the value, and around it: surrounding whitespace, so trimmed, and the lease is found.
# - The test of a trailing line feed alone is in test-id-whole-value.ps1, since a program that a .cmd
#   starts cuts that one to the same ID and passes it.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process,
#   in a throwaway home. The harness passes each argument through ProcessStartInfo.ArgumentList, so
#   a line feed reaches a program that is not a batch file whole. It reads no real folder, loads no
#   prototype code and names no prototype path. A lease ID read from the program's output names a file
#   only after the document passed its schema. It deletes nothing and writes nothing itself. A
#   failure reason never quotes the program's output.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-id-line-feed.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'lease-api.md#input-tolerance'
$unknown = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'

$lab = $null
try {
  $lab = New-ConformanceHome

  # ---- A line feed inside the value is refused, for every command that takes an ID -----------------------
  foreach ($sub in 'get', 'renew', 'release') {
    foreach ($v in @(
        @{ Text = "$unknown`nx"; Why = 'a line feed and a letter after the ID' }
        @{ Text = "$unknown`n."; Why = 'a line feed and a dot after the ID' }
        @{ Text = "$unknown`n$unknown"; Why = 'two IDs on two lines' }
        @{ Text = "$($unknown.Substring(0, 8))`n$($unknown.Substring(8))"; Why = 'a line feed inside the ID' }
      )) {
      $run = Invoke-CcLease -Lab $lab -Arguments @($sub, $v.Text)
      Complete-ConformanceCheck -Name "$sec`: $sub refuses $($v.Why) with invalid_value for id, exit 2, one error document" -Problems (Get-HardeningIdRefusalProblems $run)
    }
  }

  # ---- A real lease is not found by a value with a line feed inside it ---------------------------------------
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: get of a real lease's ID with a line feed and a letter after it is refused with invalid_value for id, and the lease is not printed" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
    Write-ConformanceResult -Name "$sec`: get trims a line feed before the ID, and whitespace and line feeds around an upper case ID, and prints the lease" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $l = New-CcReadyLease -Lab $lab -Project 'idlf'
    if (-not $l.Ok) {
      Write-ConformanceResult -Name "$sec`: Setup, a ready lease is made by create" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates"
    } else {
      $id = $l.Doc['id']
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', "$id`nx")
      Complete-ConformanceCheck -Name "$sec`: get of a real lease's ID with a line feed and a letter after it is refused with invalid_value for id, and the lease is not printed" -Problems (Get-HardeningIdRefusalProblems $run)

      $plain = Invoke-CcLease -Lab $lab -Arguments @('get', $id)
      $p = [System.Collections.Generic.List[string]]::new()
      foreach ($v in @("`n$id", " `n$($id.ToUpperInvariant()) `r`n")) {
        $run = Invoke-CcLease -Lab $lab -Arguments @('get', $v)
        $q = Get-ConformanceSuccessProblems -Run $run
        if ($q.Count -eq 0) {
          if ($run.Json['id'] -cne $id) { $q.Add('the lease printed is not the one asked for, or its ID is not the normalized one') }
          elseif ((ConvertTo-CcCanonical $run.Json) -cne (ConvertTo-CcCanonical $plain.Json)) { $q.Add('the document differs from the one the plain ID gives') }
        }
        foreach ($m in $q) { $p.Add("with $($v.Length) characters: $m") }
      }
      Complete-ConformanceCheck -Name "$sec`: get trims a line feed before the ID, and whitespace and line feeds around an upper case ID, and prints the lease" -Problems $p
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-id-line-feed.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 3 and #0029 point 2): the
# lease ID check matches the whole value. It enforces:
#   - docs/lease-api.md, "Lease ID": a value that is not a UUIDv4 once trimmed, a trailing dot
#     included, is refused with `invalid_value` for `id` (exit 2); a well-formed ID that names no
#     lease is `not_found` (exit 3), and the `lease_id` it carries is the normalized ID.
#   - docs/lease-api.md, "Input tolerance": surrounding whitespace, a line feed included, is trimmed
#     before the value is checked, and every check MUST match the whole value, so a line feed or a
#     dot after the ID never reaches a file name or a lookup (#0029).
#   - docs/lease-api.md, "Exit codes and errors": a message that quotes a value with a control
#     character writes each as `\u` and four lowercase hex digits, or leaves the value out.
# Each case is run for every command that takes an ID: get, renew and release.
#
# - A trailing line feed is surrounding whitespace, so it is trimmed and the ID is accepted. A line
#   feed INSIDE the value is not, and is refused; that case, and a line feed before the ID, are in
#   test-unmet-id-line-feed.ps1, since a program that a .cmd starts is cut at the first line feed
#   before it sees the value (docs/lease-api.md, "The program").
# - The program is the one AGENTSPACE_CMD names, run only through the harness as a separate process
#   in a throwaway home. The check reads no real folder, loads no prototype code and names no
#   prototype path. A lease ID read from the program's output names a file only after the document
#   passed its schema (Get-CcLeaseDir, Get-CcAliasPath). The check deletes nothing and writes nothing
#   itself. A failure reason never quotes the program's output.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-id-whole-value.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'lease-api.md#lease-id'
# A well-formed UUIDv4 that no lease has.
$unknown = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'

# The problems with an ID that was trimmed and accepted, as a get that finds the lease: the same lease
# as the plain ID gives, printed whole, and the normalized ID.
function Get-IdAcceptedProblems($Run, $Plain, [string]$Id) {
  $p = Get-ConformanceSuccessProblems -Run $Run
  if ($p.Count -eq 0) {
    if ($Run.Json['id'] -cne $Id) { $p.Add('The lease printed is not the one asked for, or its ID is not the normalized one') }
    elseif ((ConvertTo-CcCanonical $Run.Json) -cne (ConvertTo-CcCanonical $Plain.Json)) { $p.Add('The document differs from the one the plain ID gives') }
  }
  return , $p
}

$lab = $null
try {
  $lab = New-ConformanceHome

  # The leases the cases look at, made by `create`. Without a CA there are none, and the cases that
  # need one are reported SKIPPED; the refusals of an ID need no lease, and always run.
  $ids = @()
  if ($lab.CaReady) {
    foreach ($n in 'idwhole-a', 'idwhole-b', 'idwhole-c') {
      $l = New-CcReadyLease -Lab $lab -Project $n
      if ($l.Ok) { $ids += $l.Doc['id'] } else { Write-ConformanceResult -Name "$sec`: Setup, a ready lease is made by create ($n)" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates" }
    }
  }
  $have = $ids.Count -eq 3
  if (-not $have -and -not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: The cases that need a ready lease (a trailing dot leaves it alone, a trailing line feed is trimmed)" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  # The ID the refusals are given: a real lease's when there is one, so that a build that trims the dot
  # and then finds the lease is told apart from one that refuses it. Otherwise a well-formed unknown ID.
  $target = if ($have) { $ids[0] } else { $unknown }

  # ---- A trailing dot is refused, by every command that takes an ID --------------------------------------
  $before = if ($have) { Get-CcStateCanonical $lab } else { '' }
  foreach ($sub in 'get', 'renew', 'release') {
    foreach ($v in @(
        @{ Text = "$target."; Why = 'a dot after the ID' }
        @{ Text = "$target.."; Why = 'two dots after the ID' }
        @{ Text = "$target ."; Why = 'a space and a dot after the ID' }
        @{ Text = " $target. "; Why = 'a dot after the ID inside surrounding spaces' }
        @{ Text = ($target.ToUpperInvariant() + '.'); Why = 'a dot after an upper case ID' }
        @{ Text = "$target.`n"; Why = 'a dot and then a line feed after the ID' }
      )) {
      $run = Invoke-CcLease -Lab $lab -Arguments @($sub, $v.Text)
      Complete-ConformanceCheck -Name "$sec`: $sub refuses $($v.Why) with invalid_value for id, exit 2, one error document" -Problems (Get-HardeningIdRefusalProblems $run)
    }
  }
  if ($have) {
    # Refusing is not enough: the dot MUST NOT have been stripped on the way to the lease. The state
    # file is the same document, and the lease is still ready, with its folder and its alias file.
    $p = [System.Collections.Generic.List[string]]::new()
    if ((Get-CcStateCanonical $lab) -cne $before) { $p.Add('The state file changed') }
    $get = Invoke-CcLease -Lab $lab -Arguments @('get', $target)
    if (-not (Test-CcSuccess $get) -or $get.Json['state'] -cne 'ready') { $p.Add('The lease is no longer ready') }
    elseif (-not (Test-Path -LiteralPath (Get-CcLeaseDir $lab $target))) { $p.Add('The lease folder is gone') }
    elseif (-not (Test-Path -LiteralPath (Get-CcAliasPath $lab ([string]$get.Json['ssh']['host_alias'])) -PathType Leaf)) { $p.Add('The alias file is gone') }
    Complete-ConformanceCheck -Name "$sec`: A refused ID with a trailing dot changes nothing: the state file, the lease, its folder and its alias file are as they were" -Problems $p
  }

  # ---- A trailing line feed is surrounding whitespace: trimmed, then the lease is found --------------------
  if ($have) {
    $plain = Invoke-CcLease -Lab $lab -Arguments @('get', $ids[0])
    foreach ($v in @(
        @{ Text = "$($ids[0])`n"; Why = 'a line feed after the ID' }
        @{ Text = "$($ids[0].ToUpperInvariant())`r`n"; Why = 'a carriage return and a line feed after an upper case ID' }
      )) {
      $run = Invoke-CcLease -Lab $lab -Arguments @('get', $v.Text)
      Complete-ConformanceCheck -Name "$sec`: get with $($v.Why) trims it, normalizes the ID and prints the lease" -Problems (Get-IdAcceptedProblems $run $plain $ids[0])
    }
    # renew and release trim the same way, each on a lease of its own.
    $run = Invoke-CcLease -Lab $lab -Arguments @('renew', "$($ids[1])`n")
    $p = Get-ConformanceSuccessProblems -Run $run
    if ($p.Count -eq 0 -and ($run.Json['id'] -cne $ids[1] -or $run.Json['state'] -cne 'ready')) { $p.Add('It did not print the ready lease asked for') }
    Complete-ConformanceCheck -Name "$sec`: renew with a line feed after the ID trims it and renews that lease" -Problems $p
    $run = Invoke-CcLease -Lab $lab -Arguments @('release', "$($ids[2])`n")
    $p = Get-ConformanceSuccessProblems -Run $run
    if ($p.Count -eq 0 -and ($run.Json['id'] -cne $ids[2] -or $run.Json['state'] -cne 'released')) { $p.Add('It did not print the released lease asked for') }
    Complete-ConformanceCheck -Name "$sec`: release with a line feed after the ID trims it and releases that lease" -Problems $p
  }

  # ---- A well-formed ID that names no lease is not_found, and says the normalized ID ----------------------
  foreach ($sub in 'get', 'renew', 'release') {
    $run = Invoke-CcLease -Lab $lab -Arguments @($sub, " $($unknown.ToUpperInvariant())`n")
    $p = Get-HardeningErrorProblems -Run $run -Code 'not_found' -LeaseId $unknown
    Complete-ConformanceCheck -Name "$sec`: $sub of an unknown ID given in upper case with surrounding whitespace is not_found, exit 3, with the normalized lease_id" -Problems $p
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-id-whole-value.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

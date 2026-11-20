#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail"). The
# prototype reads such a file for `get` and `list`, and refuses it only when a changing command
# writes, after the driver has acted: 15 of its 25 results FAIL.
#
# A conformance check under D34 (E0.06-T04-S03, #0133, carrying #0110 point 4): a state file that
# names one lease ID twice is not valid. It enforces:
#   - docs/state.md, "The lease state file": two records MUST NOT share an `id`; a file that
#     validates against the schema but holds two records with one `id` is not valid, though the
#     schema cannot refuse it, and every lease command MUST fail with `internal`, "could not be
#     read", and change nothing; an object that names one member twice, such as a record with two
#     `id` members, makes the file not valid, whichever value a JSON parser would keep.
#   - docs/state.md, "Requirements the prototype does not meet yet" (a state file in which two
#     records share an `id`).
#   - docs/lease-api.md, "Exit codes and errors" and "Messages" (Records unreadable): `internal`,
#     exit 1, nothing on stdout, one error document, no absolute path.
# Each variant is run for get (the lease named twice, and the other one), list, renew, release and
# create, and each run MUST leave every file under the home as it was: no lease folder, alias file,
# credential or record is made, changed or removed. A `create` that asks the driver before it reads
# the file leaves files behind, and is caught by that.
#
# - The variants are seeded between commands from two leases made by `create`: an identical copy of
#   the first record, a record with the same `id` and a different project, and a record with an `id`
#   member twice. A lease ID read from the program's output names a file only after the document
#   passed its schema. Every write calls Assert-ConformancePath at the call (Write-CcState and the
#   raw write below), and no program runs between the call and the write. The check deletes nothing.
#   A failure reason never quotes the program's output.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate
#   process in a throwaway home. It reads no real folder, loads no prototype code and names no
#   prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-state-duplicate-id.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#the-lease-state-file'

# The problems with a run that was meant to fail because the state file is not valid: `internal`,
# exit 1, "could not be read" in the message, and no absolute path in it.
function Get-DuplicateProblems($Run) {
  $p = Get-HardeningErrorProblems -Run $Run -Code 'internal'
  $doc = $Run.ErrorJson
  if ($doc -is [hashtable]) {
    $m = [string]$doc['message']
    if ($m -cnotlike '*could not be read*') { $p.Add('The message does not say that the lease records could not be read') }
    if ($m -match '(?i)[a-z]:[\\/]') { $p.Add('The message holds an absolute path') }
  }
  return , $p
}

$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: A state file that names one lease ID twice is not valid, for every lease command" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $a = New-CcReadyLease -Lab $lab -Project 'dupid-a'
    $b = New-CcReadyLease -Lab $lab -Project 'dupid-b'
    if (-not ($a.Ok -and $b.Ok)) {
      Write-ConformanceResult -Name "$sec`: Setup, two ready leases are made by create" -Status FAIL -Reason 'create did not print a ready lease that validates, twice'
    } else {
      $idA = [string]$a.Doc['id']
      $idB = [string]$b.Doc['id']
      $base = Read-CcState $lab
      # The control: before any seeding, the first lease is read, so the failures below are the
      # duplicate's, not a program that fails every get.
      $ctl = Invoke-CcLease -Lab $lab -Arguments @('get', $idA)
      Complete-ConformanceCheck -Name "$sec`: Setup, the two leases are read before the state file is changed" -Problems $(if (Test-CcSuccess $ctl) { $null } else { [System.Collections.Generic.List[string]]@('get of a lease made by create failed') })

      # Each variant is a function that seeds the state file from the base document.
      $variants = @(
        @{ Name = 'two records with one id and the same content'; Seed = {
            $d = Read-CcState $lab
            $copy = @($d['leases'] | Where-Object { $_['id'] -ceq $idA })[0]
            $d['leases'] = @($d['leases']) + @(, ($copy | ConvertTo-Json -Depth 20 | ConvertFrom-Json -AsHashtable -DateKind String))
            Write-CcState $lab $d } }
        @{ Name = 'two records with one id and different content'; Seed = {
            $d = Read-CcState $lab
            $copy = @($d['leases'] | Where-Object { $_['id'] -ceq $idA })[0]
            $other = $copy | ConvertTo-Json -Depth 20 | ConvertFrom-Json -AsHashtable -DateKind String
            $other['project'] = 'dupid-other'
            $d['leases'] = @($d['leases']) + @(, $other)
            Write-CcState $lab $d } }
        @{ Name = 'one record with an id member twice'; Seed = {
            $path = Get-CcStatePath $lab
            $text = [IO.File]::ReadAllText($path, [Text.UTF8Encoding]::new($false))
            $needle = '"id":"' + $idA + '"'
            if (-not $text.Contains($needle)) { throw 'The state file has no id member in the compact form the check looks for.' }
            $text = $text.Replace($needle, $needle + ',' + $needle)
            [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $path), [Text.UTF8Encoding]::new($false).GetBytes($text)) } }
      )
      foreach ($v in $variants) {
        Write-CcState $lab $base                      # Start each variant from the same two leases.
        & $v.Seed
        $stateBefore = Get-CcFileHash (Get-CcStatePath $lab)
        $mapBefore = Get-HardeningFileMap $lab
        $lines = @(
          @{ Name = "get of the lease named twice"; Args = @('get', $idA) }
          @{ Name = "get of the other lease"; Args = @('get', $idB) }
          @{ Name = 'list'; Args = @('list') }
          @{ Name = 'renew of the lease named twice'; Args = @('renew', $idA) }
          @{ Name = 'release of the lease named twice'; Args = @('release', $idA) }
          @{ Name = 'release of the other lease'; Args = @('release', $idB) }
          @{ Name = 'a ready create'; Args = @('create', '--project', 'dupid-new') }
        )
        foreach ($l in $lines) {
          $run = Invoke-CcLease -Lab $lab -Arguments $l.Args
          Complete-ConformanceCheck -Name "$sec`: With $($v.Name), $($l.Name) fails with internal, exit 1, and says the records could not be read" -Problems (Get-DuplicateProblems $run)
        }
        $p = [System.Collections.Generic.List[string]]::new()
        if ((Get-CcFileHash (Get-CcStatePath $lab)) -ne $stateBefore) { $p.Add('The state file changed') }
        foreach ($d in (Get-HardeningMapDifferences -Before $mapBefore -After (Get-HardeningFileMap $lab))) { $p.Add($d) }
        Complete-ConformanceCheck -Name "$sec`: With $($v.Name), the seven commands changed nothing: not the state file, a lease folder, a credential, an alias file or the config" -Problems $p
      }
    }
  }
} catch {
  $why = ($_.Exception.Message -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-state-duplicate-id.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

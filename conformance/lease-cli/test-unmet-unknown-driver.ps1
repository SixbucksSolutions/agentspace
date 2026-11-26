#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# EXPECTED TO FAIL against the prototype. A rewrite MUST pass it. This is not a regression when it
# fails against the prototype (docs/conformance.md, "The files that are expected to fail").
#
# The requirement (#0041 point 2; D1), MUSTs: a `renew` or `release` of a record whose `driver` names
# no driver the reader knows MUST fail with `internal`, change nothing, and its message MUST NOT name
# the stored driver; and the stored driver's name MUST NOT reach any output.
#   - docs/state.md, "The lease state file" (The driver is kept for as long as the lease is: a record
#     whose `driver` names no driver the reader knows) and "Requirements the prototype does not meet
#     yet" (an unknown driver's message).
#   - docs/driver.md, "The driver's name" and "What a driver never exposes": the driver's own name.
#   - docs/lease-api.md, "Messages" (Unknown stored driver: the message never names the driver).
# Card: E0.06-T04-S03 (#0133), carrying #0110 point 4. What the prototype does instead: its message
# names the stored driver (docs/state.md).
#
# - The stored driver is a made-up name that no build can know, written into one ready record of the
#   state file between commands: the state schema requires only a non-empty string with no control
#   character. The cases run `renew` and `release` of that lease (each on a lease of its own, since
#   a program that did handle it would change it), then `get` and `list`, which MUST NOT print the
#   name. Each failing run MUST leave every file under the home as it was.
# - A lease ID read from the program's output names a file only after the document passed its schema.
#   The one write calls Assert-ConformancePath at the call, through Write-CcState, with no program run
#   between the call and the write. The check deletes nothing. A failure reason never quotes the
#   program's output.
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate process
#   in a throwaway home. It reads no real folder, loads no prototype code and names no prototype path.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-unmet-unknown-driver.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')
. (Join-Path $PSScriptRoot 'lease-check-hardening.ps1')

$sec = 'state.md#the-lease-state-file'
# A name no driver has. It is a made-up word, and a run is searched for it in every case.
$driverName = 'zq-conformance-driver-x9'

# True when neither stream of the run holds the name, in any case (D1: it MUST NOT reach any output).
function Test-NoDriverName($Run) {
  -not (([string]$Run.Stdout).IndexOf($driverName, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or ([string]$Run.Stderr).IndexOf($driverName, [StringComparison]::OrdinalIgnoreCase) -ge 0)
}

$lab = $null
try {
  $lab = New-ConformanceHome
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$sec`: A record whose driver names no known driver fails renew and release with internal" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  } else {
    $ids = @{}
    foreach ($n in 'renew', 'release') {
      $l = New-CcReadyLease -Lab $lab -Project "unkdrv-$n"
      if ($l.Ok) { $ids[$n] = [string]$l.Doc['id'] } else { Write-ConformanceResult -Name "$sec`: Setup, a ready lease is made by create for the $n" -Status FAIL -Reason "create exited $($l.Run.ExitCode), or did not print a ready lease that validates" }
    }
    if ($ids.Count -eq 2) {
      # Give both records the made-up driver, and check the file still validates against its schema.
      $d = Read-CcState $lab
      foreach ($r in $d['leases']) { $r['driver'] = $driverName }
      Write-CcState $lab $d
      $text = [IO.File]::ReadAllText((Get-CcStatePath $lab), [Text.UTF8Encoding]::new($false))
      Complete-ConformanceCheck -Name "$sec`: Setup, the state file with an unknown driver in both records still validates against its schema" `
        -Problems $(if (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/leases.state.schema.json') { $null } else { [System.Collections.Generic.List[string]]@('It does not validate, so the seed is not a valid file') })

      foreach ($sub in 'renew', 'release') {
        $id = $ids[$sub]
        $stateBefore = Get-CcFileHash (Get-CcStatePath $lab)
        $mapBefore = Get-HardeningFileMap $lab
        $run = Invoke-CcLease -Lab $lab -Arguments @($sub, $id)
        $p = Get-HardeningErrorProblems -Run $run -Code 'internal'
        Complete-ConformanceCheck -Name "$sec`: $sub of a lease whose driver is unknown fails with internal, exit 1, one error document" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        if (-not (Test-NoDriverName $run)) { $p.Add('The stored driver''s name is in the output') }
        Complete-ConformanceCheck -Name "$sec`: The message of $sub names no driver, and the stored driver's name is in no output (D1)" -Problems $p

        $p = [System.Collections.Generic.List[string]]::new()
        if ((Get-CcFileHash (Get-CcStatePath $lab)) -ne $stateBefore) { $p.Add('The state file changed') }
        foreach ($x in (Get-HardeningMapDifferences -Before $mapBefore -After (Get-HardeningFileMap $lab))) { $p.Add($x) }
        Complete-ConformanceCheck -Name "$sec`: A failed $sub of a lease whose driver is unknown changes nothing: the record, the credential, the alias file and the config are as they were" -Problems $p
      }

      # The name never reaches get or list either, whatever they print.
      $p = [System.Collections.Generic.List[string]]::new()
      # Each also MUST succeed, with a document that validates: the driver is the state file's own, and
      # `get` and `list` need none (a program that fails them has not read the record without the driver).
      $reads = @(
        @{ Name = 'get'; Arguments = @('get', $ids['renew']); Schema = 'lease.output.schema.json' }
        @{ Name = 'list'; Arguments = @('list'); Schema = 'lease-list.output.schema.json' }
      )
      foreach ($c in $reads) {
        $run = Invoke-CcLease -Lab $lab -Arguments $c.Arguments
        if (-not (Test-NoDriverName $run)) { $p.Add("The stored driver's name is in the output of $($c.Name)") }
        if (-not (Test-CcSuccess $run -Schema $c.Schema)) { $p.Add("$($c.Name) did not succeed with a document that validates (exit $($run.ExitCode))") }
      }
      Complete-ConformanceCheck -Name "$sec`: get and list of a lease whose driver is unknown succeed, and print the stored driver's name in neither stream (D1)" -Problems $p
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-unknown-driver.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($lab) { Remove-ConformanceHome -ConformanceHome $lab }
}
Exit-ConformanceRun

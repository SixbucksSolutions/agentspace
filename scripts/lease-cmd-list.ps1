#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease list (docs/lease-api.md, "agentspace lease list"). Story E1.02 wrote the stub,
# and story E1.05 filled it in, without changing scripts\lease-cli.ps1.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseList -Project <string> -State <string[]>
# - lease-cli.ps1 has already parsed the command line and normalized and checked every value with
#   scripts\lease-values.ps1, in the order of the contract's table. Every usage and invalid_value
#   error has therefore been reported before this runs.
# - -Project is a normalized project name, or empty when --project was not given. -State holds the
#   normalized state names without duplicates, or is empty when --state was not given. Either one
#   empty means no filter on it.
# - It returns the document to print: [ordered]@{ leases = @(<lease documents>) }, each as
#   ConvertTo-LeaseOutput returns it, with the array kept an array when it holds one lease or none.
#   lease-cli.ps1 writes it with ConvertTo-Json -Compress -Depth 20, plus one line feed, and exits 0.
# - It fails by throwing. A lease error (New-LeaseError in scripts\lease-values.ps1) is printed as
#   it is, with the exit code of its code. Any other exception becomes internal (exit 1) with the
#   exception's message, which therefore MUST NOT name a backend, a location or a machine (D1).
# - lease-cli.ps1 dot-sources scripts\lease-values.ps1 and scripts\lease-state.ps1 before this file,
#   so their functions are available: the state file (Read-LeaseState, Update-LeaseState,
#   Find-LeaseRecord, ConvertTo-LeaseOutput) and the times (Get-LeaseNow, ConvertTo-LeaseTime).
#   This file MUST NOT normalize values itself; anything it needs MUST come from lease-values.ps1.
#   It MAY dot-source scripts\lease-driver.ps1 (story E1.03) for the driver interface.
#
# How it works (story E1.05): it reads the state file without the lock, turns each record into its
# lease document as of one instant, filters on the project (exact) and on the derived state, and
# sorts by created_at then id, both ordinal, so the order does not depend on the culture.

function Invoke-LeaseList {
  param([string]$Project = '', [string[]]$State = @())
  $now = Get-LeaseNow
  $docs = [System.Collections.Generic.List[object]]::new()
  foreach ($record in (Read-LeaseState)) {
    $doc = ConvertTo-LeaseOutput -Record $record -Now $now
    if ($Project -and $doc['project'] -cne $Project) { continue }
    if ($State.Count -gt 0 -and $State -cnotcontains $doc['state']) { continue }
    $docs.Add($doc)
  }
  $docs.Sort([Comparison[object]]{
    param($a, $b)
    $c = [string]::CompareOrdinal($a['created_at'], $b['created_at'])
    if ($c -ne 0) { $c } else { [string]::CompareOrdinal($a['id'], $b['id']) }
  })
  [ordered]@{ leases = $docs.ToArray() }
}

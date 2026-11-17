#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease list (docs/lease-api.md, "agentspace lease list"). Story E1.02 wrote this stub;
# story E1.05 fills it in, and MUST NOT need to change scripts\lease-cli.ps1 to do so.
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
# Until then it answers internal (exit 1): the contract has no "not implemented" error, and no
# other exit code MAY be used.

function Invoke-LeaseList {
  param([string]$Project = '', [string[]]$State = @())
  throw (New-LeaseError -Code internal -Message 'agentspace lease list is not implemented yet.')
}

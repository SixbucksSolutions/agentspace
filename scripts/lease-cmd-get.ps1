#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease get (docs/lease-api.md, "agentspace lease get"). Story E1.02 wrote the stub,
# and story E1.04 filled it in, without changing scripts\lease-cli.ps1.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseGet -LeaseId <string>
# - lease-cli.ps1 has already parsed the command line and normalized and checked the ID with
#   scripts\lease-values.ps1, so every usage and invalid_value error has been reported before this
#   runs.
# - -LeaseId is a lowercase UUIDv4. An ID that names no lease MUST be thrown as not_found, with
#   -LeaseId set.
# - It returns the document to print: the lease document, as ConvertTo-LeaseOutput returns it.
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
function Invoke-LeaseGet {
  param([Parameter(Mandatory)][string]$LeaseId)
  $records = Read-LeaseState
  $record = Find-LeaseRecord -Records $records -LeaseId $LeaseId
  if (-not $record) {
    throw (New-LeaseError -Code not_found -LeaseId $LeaseId -Message "No lease has ID '$LeaseId'.")
  }
  ConvertTo-LeaseOutput -Record $record
}

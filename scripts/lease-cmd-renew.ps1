#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease renew (docs/lease-api.md, "agentspace lease renew"). Story E1.02 wrote the stub,
# and story E1.05 filled it in, without changing scripts\lease-cli.ps1.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseRenew -LeaseId <string> -TtlSeconds <int>
# - lease-cli.ps1 has already parsed the command line and normalized and checked every value with
#   scripts\lease-values.ps1, in the order of the contract's table. Every usage, invalid_value and
#   out_of_range error has therefore been reported before this runs.
# - -LeaseId is a lowercase UUIDv4. -TtlSeconds is from 60 to 86400, or 0 when --ttl was not given,
#   which means the lease's current ttl_seconds (D24). not_found and then invalid_state are this
#   function's to throw, in that order (docs/lease-api.md, "Exit codes and errors").
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
# How it works (story E1.05):
# - The lookup, the driver call and the record change are one Update-LeaseState step, so they are
#   atomic under the state lock. A driver failure throws out of the block and the record stays as it
#   was. The block runs in a child scope, so it changes the record object in place and assigns to no
#   variable outside the block.
# - Only a lease that ConvertTo-LeaseOutput reports as ready is renewed; any other state, expired
#   included, is invalid_state with that state.
# - The driver gets the record's stored driver name and project. expires_at is the certificate's
#   valid-before time that the driver returns.

. (Join-Path $PSScriptRoot 'lease-driver.ps1')

function Invoke-LeaseRenew {
  param([Parameter(Mandatory)][string]$LeaseId, [int]$TtlSeconds = 0)
  $leaseIdValue = $LeaseId
  $ttlValue = $TtlSeconds
  Update-LeaseState -Script {
    param($Records)
    $record = Find-LeaseRecord -Records $Records -LeaseId $leaseIdValue
    if (-not $record) {
      throw (New-LeaseError -Code not_found -LeaseId $leaseIdValue -Message "No lease has the ID '$leaseIdValue'.")
    }
    $state = (ConvertTo-LeaseOutput -Record $record)['state']
    if ($state -cne 'ready') {
      $advice = switch ($state) {
        'expired' { 'Create a new lease.' }
        'pending' { 'Wait for it to be ready.' }
        default { "The $state state is final." }
      }
      throw (New-LeaseError -Code invalid_state -LeaseId $leaseIdValue -State $state -Message "Lease '$leaseIdValue' is $state, and only a ready lease can be renewed. $advice")
    }
    $ttl = if ($ttlValue -gt 0) { $ttlValue } else { [int]$record['ttl_seconds'] }
    $renewed = Invoke-LeaseDriverRenew -Driver $record['driver'] -LeaseId $leaseIdValue -Project $record['project'] -Ttl ([TimeSpan]::FromSeconds($ttl))
    $record['expires_at'] = ConvertTo-LeaseTime $renewed.ExpiresAt
    $record['ttl_seconds'] = $ttl
    ConvertTo-LeaseOutput -Record $record
  }
}

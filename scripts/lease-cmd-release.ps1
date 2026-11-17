#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease release (docs/lease-api.md, "agentspace lease release"). Story E1.02 wrote the
# stub, and story E1.05 filled it in, without changing scripts\lease-cli.ps1.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseRelease -LeaseId <string>
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
# How it works (story E1.05):
# - The lookup, the driver call and the record change are one Update-LeaseState step, so they are
#   atomic under the state lock. A driver failure throws out of the block and the record stays as it
#   was. The block changes the record object in place and assigns to no variable outside the block.
# - released and unsatisfiable come back unchanged, so the first released_at stays. A pending lease
#   has no credential, so the driver is not called. A ready or expired lease is torn down through the
#   driver, then marked released with ssh dropped and expires_at kept.
# - The environment variable AGENTSPACE_SSH_DIR, when set, is passed to the driver as -SshDir; tests
#   MUST set it to a throwaway folder.

. (Join-Path $PSScriptRoot 'lease-driver.ps1')

function Invoke-LeaseRelease {
  param([Parameter(Mandatory)][string]$LeaseId)
  $leaseIdValue = $LeaseId
  Update-LeaseState -Script {
    param($Records)
    $record = Find-LeaseRecord -Records $Records -LeaseId $leaseIdValue
    if (-not $record) {
      throw (New-LeaseError -Code not_found -LeaseId $leaseIdValue -Message "No lease has the ID '$leaseIdValue'.")
    }
    $stored = $record['state']
    if ($stored -ceq 'ready') {
      $release = @{ Driver = $record['driver']; LeaseId = $leaseIdValue; Project = $record['project'] }
      if ($env:AGENTSPACE_SSH_DIR) { $release.SshDir = $env:AGENTSPACE_SSH_DIR }
      Invoke-LeaseDriverRelease @release
    }
    if ($stored -ceq 'ready' -or $stored -ceq 'pending') {
      $record['state'] = 'released'
      $record['released_at'] = ConvertTo-LeaseTime (Get-LeaseNow)
      $record.Remove('ssh')
    }
    ConvertTo-LeaseOutput -Record $record
  }
}

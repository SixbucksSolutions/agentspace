#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# agentspace lease create (docs/lease-api.md, "agentspace lease create"). Story E1.02 wrote this
# stub; story E1.04 fills it in, and MUST NOT need to change scripts\lease-cli.ps1 to do so.
#
# The function that scripts\lease-cli.ps1 calls, which MUST keep this name and these parameters:
#   Invoke-LeaseCreate -Project <string> -Platform <string> -Profile <string> -Size <string> -TtlSeconds <int>
# - lease-cli.ps1 has already parsed the command line, applied the defaults, and normalized and
#   checked every value with scripts\lease-values.ps1, in the order of the contract's table. Every
#   usage, invalid_value and out_of_range error has therefore been reported before this runs.
# - Every argument is normalized and has its default applied: -Platform linux/arm64, -Profile
#   agent-linux, -Size m and -TtlSeconds 28800 (8 hours, D24) when the flag was not given.
#   -TtlSeconds is from 60 to 86400; the driver takes [TimeSpan]::FromSeconds($TtlSeconds).
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
# Until then it answers internal (exit 1): the contract has no "not implemented" error, and no
# other exit code MAY be used.

function Invoke-LeaseCreate {
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$Platform,
    [Parameter(Mandatory)][string]$Profile,
    [Parameter(Mandatory)][string]$Size,
    [Parameter(Mandatory)][int]$TtlSeconds
  )
  throw (New-LeaseError -Code internal -Message 'agentspace lease create is not implemented yet.')
}

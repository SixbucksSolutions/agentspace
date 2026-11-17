#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The driver interface (D6): one contract for every backend. A driver places a lease and returns its
# SSH endpoint, renews it, and tears it down. The lease CLI calls only the functions below, and it
# MUST NOT name or compare a driver. This file MUST NOT name any backend, region or machine (D1).
#
# Dot-sourcing this file dot-sources every lease-driver-*.ps1 beside it. Each of those registers
# itself with Register-LeaseDriver. A driver's name is an opaque string that the CLI stores and
# hands back on renew and release.
#
# Failure is a thrown exception, and its message MUST NOT name a backend, a region or a machine.
# A failed place MUST leave no credential and no alias behind.
#
# THE INTERFACE (written for the lease CLI, E1.04 and E1.05, which wire to it)
#
#   Register-LeaseDriver -Name <string> -Place <scriptblock> -Renew <scriptblock> -Release <scriptblock>
#     Called by each lease-driver-*.ps1 file as it loads. Each scriptblock takes one [hashtable] of
#     the named arguments below and returns a [pscustomobject]. The name is opaque: the CLI stores it
#     in the state file, hands it back on renew and release, and MUST NOT compare it to a literal.
#
#   Invoke-LeaseDriverPlace -LeaseId <string> -Project <string> -Platform <string> -Profile <string>
#       -Size <string> -Ttl <TimeSpan> [-SshDir <string>] [-ProxyCommand <string>]
#     Places a lease. Version 0 asks the first registered driver. Returns [pscustomobject]:
#       Driver     string    the driver's name, to store and pass back later
#       State      string    'ready' or 'unsatisfiable' (a later driver MAY answer 'pending')
#       HostAlias  string    ready only
#       User       string    ready only
#       ExpiresAt  DateTime  ready only; UTC, whole second, equal to the certificate's valid-before
#       Reasons    object[]  unsatisfiable only; each [pscustomobject]@{ code; message }, in the
#                            contract's table order, every one that applies (docs/lease-api.md)
#
#   Invoke-LeaseDriverRenew -Driver <string> -LeaseId <string> -Project <string> -Ttl <TimeSpan>
#     Returns [pscustomobject]@{ ExpiresAt = <DateTime, UTC, whole second> }. Replaces only the
#     certificate; the key is unchanged.
#
#   Invoke-LeaseDriverRelease -Driver <string> -LeaseId <string> -Project <string> [-SshDir <string>]
#     Deletes the key, certificate and alias. Idempotent. Returns nothing the CLI relies on.
#
# THE RULES
#   - Inputs are already normalized by the CLI (lowercase UUIDv4 ID, lowercase project, normalized
#     platform, profile and size, TTL in whole seconds from 60 to 86400). A driver MAY re-check them
#     and MUST NOT re-normalize them differently.
#   - Failure is a thrown exception, which the CLI turns into the internal error (exit 1). A driver's
#     message MUST NOT name the backend, a region or a machine, because the CLI MAY put it in an error
#     document (D1). A driver wraps the underlying error when that error would.
#   - A failed place MUST leave no credential and no alias behind (docs/lease-api.md, "Concurrency and
#     atomicity"). A driver that fails after issuing the credential MUST remove it before it rethrows.
#   - Unsatisfiable is a returned state, not an exception, and issues nothing. Version 0 supports only
#     linux/arm64, agent-linux with no version, and size m (D5); each unmet value is one reason, with
#     the code unsupported_platform, unsupported_profile or unsupported_size, in that order.
#   - The CLI MUST put only the thrown exception's own Message in an error document, never its
#     inner exception or ToString(): a driver wraps an error that names a machine, and keeps it
#     as the inner exception.
#   - The driver's name MUST NOT reach any output. It is stored in the state file only.
#   - The CLI MUST pass renew and release the project stored at place, and MUST choose a lease ID
#     whose host alias no kept lease uses before it calls place (docs/lease-api.md, "Fields"). A
#     driver reports an alias collision only as a failed place.
#   - -SshDir and -ProxyCommand are for tests and worktrees. Unset, they take the defaults of
#     Set-LeaseAlias in lease-alias.ps1.

$script:LeaseDrivers = [ordered]@{}

function Register-LeaseDriver {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][scriptblock]$Place,
    [Parameter(Mandatory)][scriptblock]$Renew,
    [Parameter(Mandatory)][scriptblock]$Release
  )
  # Each scriptblock takes one [hashtable] of named arguments and returns a [pscustomobject].
  $script:LeaseDrivers[$Name] = [pscustomobject]@{ Name = $Name; Place = $Place; Renew = $Renew; Release = $Release }
}

function Get-LeaseDriver([string]$Name) {
  if ($Name) {
    if (-not $script:LeaseDrivers.Contains($Name)) { throw "No lease driver is registered as '$Name'." }
    return $script:LeaseDrivers[$Name]
  }
  # Version 0 asks the first registered driver; there is one.
  $first = $script:LeaseDrivers.Values | Select-Object -First 1
  if (-not $first) { throw 'No lease driver is registered.' }
  $first
}

function Invoke-LeaseDriverPlace {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$LeaseId,
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$Platform,
    [Parameter(Mandatory)][string]$Profile,
    [Parameter(Mandatory)][string]$Size,
    [Parameter(Mandatory)][TimeSpan]$Ttl,
    [string]$SshDir,
    [string]$ProxyCommand
  )
  $d = Get-LeaseDriver
  $r = & $d.Place @{
    LeaseId = $LeaseId; Project = $Project; Platform = $Platform; Profile = $Profile; Size = $Size
    Ttl = $Ttl; SshDir = $SshDir; ProxyCommand = $ProxyCommand
  }
  $r | Add-Member -NotePropertyName Driver -NotePropertyValue $d.Name -Force -PassThru
}

function Invoke-LeaseDriverRenew {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Driver,
    [Parameter(Mandatory)][string]$LeaseId,
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][TimeSpan]$Ttl
  )
  & (Get-LeaseDriver $Driver).Renew @{ LeaseId = $LeaseId; Project = $Project; Ttl = $Ttl }
}

function Invoke-LeaseDriverRelease {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Driver,
    [Parameter(Mandatory)][string]$LeaseId,
    [Parameter(Mandatory)][string]$Project,
    [string]$SshDir
  )
  & (Get-LeaseDriver $Driver).Release @{ LeaseId = $LeaseId; Project = $Project; SshDir = $SshDir } | Out-Null
}

foreach ($f in Get-ChildItem -LiteralPath $PSScriptRoot -Filter 'lease-driver-*.ps1' | Sort-Object Name) {
  . $f.FullName
}

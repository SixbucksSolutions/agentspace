#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The value normalizers of the lease contract (docs/lease-api.md, "Values"), its time form
# ("Times"), and the lease error that every lease script throws ("Exit codes and errors").
# Story E1.02 owns this file. The command files (scripts\lease-cmd-*.ps1) MUST call these functions
# and MUST NOT write their own normalizers, so that the T7 stories never duplicate them.
# Laptop only: nothing here touches the box or AWS.
#
# Every ConvertTo-Lease* function takes the raw value as given, and returns its normalized form or
# throws a lease error with code invalid_value or out_of_range and the field at fault.
# Matching uses -cmatch and [0-9], never -match or \d: -match ignores case, which lets characters
# such as the Kelvin sign through as a letter, and \d matches digits of every script.

# --- Lease errors -------------------------------------------------------------------------------

# Returns an exception that carries a lease error. Throw it: throw (New-LeaseError ...).
# Code is one of the contract's error codes; Field, LeaseId and State are set where the contract
# requires them. lease-cli.ps1 turns it into the error document and the exit code.
function New-LeaseError {
  param(
    [Parameter(Mandatory)][ValidateSet('internal', 'usage', 'invalid_value', 'out_of_range', 'not_found', 'invalid_state')]
    [string]$Code,
    [Parameter(Mandatory)][string]$Message,
    [string]$Field,
    [string]$LeaseId,
    [string]$State
  )
  $e = [System.Exception]::new($Message)
  $e.Data['agentspace.lease.code'] = $Code
  if ($Field) { $e.Data['agentspace.lease.field'] = $Field }
  if ($LeaseId) { $e.Data['agentspace.lease.lease_id'] = $LeaseId }
  if ($State) { $e.Data['agentspace.lease.state'] = $State }
  $e
}

# Returns the lease error an exception carries, itself or as an inner exception, or $null.
function Get-LeaseErrorException([System.Exception]$Exception) {
  for ($x = $Exception; $x; $x = $x.InnerException) {
    if ($x.Data -and $x.Data.Contains('agentspace.lease.code')) { return $x }
  }
  $null
}

# Quotes a value for a message, writing each control character as \u and four hex digits, as the
# contract requires, so that a message never carries one.
function Format-LeaseQuoted([string]$Value) {
  $sb = [System.Text.StringBuilder]::new()
  foreach ($ch in $Value.ToCharArray()) {
    $n = [int]$ch
    if ($n -le 0x1f -or $n -eq 0x7f) { [void]$sb.AppendFormat('\u{0:x4}', $n) } else { [void]$sb.Append($ch) }
  }
  "'" + $sb.ToString() + "'"
}

# --- Times --------------------------------------------------------------------------------------

# The current time, UTC, truncated to the whole second.
function Get-LeaseNow {
  $t = [DateTime]::UtcNow
  [DateTime]::new($t.Ticks - ($t.Ticks % [TimeSpan]::TicksPerSecond), [DateTimeKind]::Utc)
}

# Writes a time as YYYY-MM-DDTHH:MM:SSZ, in UTC, truncated to the whole second.
function ConvertTo-LeaseTime([Parameter(Mandatory)][DateTime]$Time) {
  $u = $Time.ToUniversalTime()
  $u = [DateTime]::new($u.Ticks - ($u.Ticks % [TimeSpan]::TicksPerSecond), [DateTimeKind]::Utc)
  $u.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
}

# Reads a time written YYYY-MM-DDTHH:MM:SSZ into a UTC DateTime. Any other form throws.
function ConvertFrom-LeaseTime([Parameter(Mandatory)][string]$Text) {
  [DateTime]::ParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal)
}

# --- Values -------------------------------------------------------------------------------------

# Lease ID: a UUIDv4 in any case, with surrounding whitespace; normalized to lowercase.
function ConvertTo-LeaseIdValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  if ($v -cnotmatch '\A[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-4[0-9A-Fa-f]{3}-[89ABab][0-9A-Fa-f]{3}-[0-9A-Fa-f]{12}\z') {
    throw (New-LeaseError -Code invalid_value -Field id -Message "Lease ID $(Format-LeaseQuoted $Value) is not a UUIDv4. Give the id that create printed.")
  }
  $v.ToLowerInvariant()
}

# Project: 1 to 32 letters, digits and '-', starting and ending with a letter or digit; lowercased.
function ConvertTo-LeaseProjectValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  if ($v -cnotmatch '\A[A-Za-z0-9]([A-Za-z0-9-]{0,30}[A-Za-z0-9])?\z') {
    throw (New-LeaseError -Code invalid_value -Field project -Message "Project $(Format-LeaseQuoted $Value) is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit.")
  }
  $v.ToLowerInvariant()
}

# Platform: os/arch or os/arch/variant; aarch64 is written arm64, x86_64 and x86-64 amd64, and the
# variant v8 of arm64 is dropped.
function ConvertTo-LeasePlatformValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  if ($v -cnotmatch '\A(?<os>[A-Za-z0-9]+)/(?<arch>[A-Za-z0-9]+|[xX]86[_-]64)(/(?<variant>[A-Za-z0-9]+))?\z') {
    throw (New-LeaseError -Code invalid_value -Field platform -Message "Platform $(Format-LeaseQuoted $Value) is not an os/arch pair. Give one as in 'linux/arm64'.")
  }
  $os = $Matches['os'].ToLowerInvariant()
  $arch = $Matches['arch'].ToLowerInvariant()
  $variant = if ($Matches['variant']) { $Matches['variant'].ToLowerInvariant() } else { '' }
  if ($arch -ceq 'aarch64') { $arch = 'arm64' }
  if ($arch -ceq 'x86_64' -or $arch -ceq 'x86-64') { $arch = 'amd64' }
  if ($arch -ceq 'arm64' -and $variant -ceq 'v8') { $variant = '' }
  if ($variant) { "$os/$arch/$variant" } else { "$os/$arch" }
}

# Profile: name or name@version; lowercased, with leading zeros dropped from the version.
function ConvertTo-LeaseProfileValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  $ok = $v -cmatch '\A(?<name>[A-Za-z0-9]([A-Za-z0-9-]{0,62}[A-Za-z0-9])?)(@(?<version>[0-9]+))?\z'
  $version = if ($ok -and $Matches['version']) { $Matches['version'].TrimStart('0') } else { $null }
  if (-not $ok -or ($null -ne $version -and $version -eq '')) {
    throw (New-LeaseError -Code invalid_value -Field profile -Message "Profile $(Format-LeaseQuoted $Value) is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more.")
  }
  $name = $Matches['name'].ToLowerInvariant()
  if ($version) { "$name@$version" } else { $name }
}

# Size: 1 to 8 letters and digits; lowercased.
function ConvertTo-LeaseSizeValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  if ($v -cnotmatch '\A[A-Za-z0-9]{1,8}\z') {
    throw (New-LeaseError -Code invalid_value -Field size -Message "Size $(Format-LeaseQuoted $Value) is not a size. Use 1 to 8 letters and digits, as in 'm'.")
  }
  $v.ToLowerInvariant()
}

# TTL: the short form or ISO 8601 (docs/lease-api.md, "TTL"), normalized to whole seconds from 60
# to 86400. A malformed TTL is invalid_value; a well-formed one outside the range, including one
# too large to compute, is out_of_range. The arithmetic is in BigInteger, so nothing overflows.
$script:LeaseTtlShort = '\A(?:(?<d>[0-9]+)\s*(?:[dD](?:[aA][yY][sS]?)?)\s*)?' +
  '(?:(?<h>[0-9]+)\s*(?:[hH](?:[oO][uU][rR][sS]?|[rR][sS]?)?)\s*)?' +
  '(?:(?<m>[0-9]+)\s*(?:m|[mM][iI][nN](?:[sS]|[uU][tT][eE][sS]?)?)\s*)?' +
  '(?:(?<s>[0-9]+)\s*(?:[sS](?:[eE][cC](?:[sS]|[oO][nN][dD][sS]?)?)?)\s*)?\z'
$script:LeaseTtlIso = '\A[pP](?=[0-9tT])(?:(?<d>[0-9]+)[dD])?(?:[tT](?=[0-9])(?:(?<h>[0-9]+)[hH])?(?:(?<m>[0-9]+)[mM])?(?:(?<s>[0-9]+)[sS])?)?\z'

function ConvertTo-LeaseTtlValue([Parameter(Mandatory)][AllowEmptyString()][string]$Value) {
  $v = $Value.Trim()
  $m = [regex]::Match($v, $script:LeaseTtlIso)
  if (-not $m.Success -and $v -ne '') {
    $m = [regex]::Match($v, $script:LeaseTtlShort)
  }
  if (-not $m.Success -or -not ($m.Groups['d'].Success -or $m.Groups['h'].Success -or $m.Groups['m'].Success -or $m.Groups['s'].Success)) {
    $q = Format-LeaseQuoted $Value
    $msg = if ($v -cmatch '\A[0-9]+\z') { "TTL $q has no unit. Give one, as in '8h'." }
    elseif ($v -cmatch '[0-9]\s*M\z|[0-9]\s*M[^A-Za-z]') { "TTL $q uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'." }
    else { "TTL $q is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'." }
    throw (New-LeaseError -Code invalid_value -Field ttl -Message $msg)
  }
  $total = [System.Numerics.BigInteger]::Zero
  foreach ($unit in @(@('d', 86400), @('h', 3600), @('m', 60), @('s', 1))) {
    $g = $m.Groups[$unit[0]]
    if ($g.Success) { $total += [System.Numerics.BigInteger]::Parse($g.Value, [Globalization.CultureInfo]::InvariantCulture) * $unit[1] }
  }
  if ($total -lt 60 -or $total -gt 86400) {
    throw (New-LeaseError -Code out_of_range -Field ttl -Message "TTL $(Format-LeaseQuoted $Value) is outside the allowed range. Give from 1m to 24h; it is never clamped.")
  }
  [int]$total
}

# State filter: each value is one state name or a comma-separated list of them, in any case, with
# surrounding whitespace. Returns the lowercase names, first seen first, with duplicates dropped.
function ConvertTo-LeaseStateFilterValue([Parameter(Mandatory)][AllowEmptyString()][string[]]$Value) {
  $names = 'pending', 'ready', 'expired', 'released', 'unsatisfiable'
  $out = [System.Collections.Generic.List[string]]::new()
  foreach ($raw in $Value) {
    foreach ($part in $raw.Split(',')) {
      $s = $part.Trim()
      if ($s -cmatch '\A[A-Za-z]+\z') { $s = $s.ToLowerInvariant() }
      if ($names -cnotcontains $s) {
        throw (New-LeaseError -Code invalid_value -Field state -Message "State $(Format-LeaseQuoted $part.Trim()) is not a state. Use pending, ready, expired, released or unsatisfiable.")
      }
      if (-not $out.Contains($s)) { $out.Add($s) }
    }
  }
  , $out.ToArray()
}

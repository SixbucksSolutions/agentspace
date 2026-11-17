#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# The EC2 pooled driver (D6): the shared box. It is the ONLY file that MAY name EC2 (D1), and it
# registers itself with the interface in lease-driver.ps1. It places a lease by issuing the lease's
# credential (E2.05) and writing its host alias (E2.06), renews by re-signing the certificate
# (E2.07), and releases by removing the alias and the credential (E2.07).
# Laptop only: nothing here touches the box or AWS.
#
# Version 0 supports linux/arm64, agent-linux (no version) and size m only (D5). Any other request
# is unsatisfiable, with one reason per unmet value, and nothing is issued.

. (Join-Path $PSScriptRoot 'lease-credential.ps1')

function Get-Ec2DriverUnmet {
  param([string]$Platform, [string]$Profile, [string]$Size)
  $reasons = @()
  if ($Platform -ne 'linux/arm64') {
    $reasons += [pscustomobject]@{ code = 'unsupported_platform'; message = "Platform '$Platform' is not supported; v0 supports linux/arm64 only." }
  }
  if ($Profile -ne 'agent-linux') {
    $reasons += [pscustomobject]@{ code = 'unsupported_profile'; message = "Profile '$Profile' is not supported; v0 supports agent-linux with no version only." }
  }
  if ($Size -ne 'm') {
    $reasons += [pscustomobject]@{ code = 'unsupported_size'; message = "Size '$Size' is not supported; v0 supports size m only." }
  }
  , $reasons
}

function Invoke-Ec2DriverPlace([hashtable]$Request) {
  $ErrorActionPreference = 'Stop'
  $unmet = Get-Ec2DriverUnmet -Platform $Request.Platform -Profile $Request.Profile -Size $Request.Size
  if ($unmet.Count -gt 0) {
    return [pscustomobject]@{ State = 'unsatisfiable'; Reasons = $unmet }
  }

  $id = ConvertTo-LeaseId $Request.LeaseId
  $cred = New-LeaseCredential -LeaseId $id -Ttl $Request.Ttl
  try {
    $alias = @{ Project = $Request.Project; LeaseId = $id }
    if ($Request.SshDir) { $alias.SshDir = $Request.SshDir }
    if ($Request.ProxyCommand) { $alias.ProxyCommand = $Request.ProxyCommand }
    $set = Set-LeaseAlias @alias
  }
  catch {
    # A failed place MUST leave no credential and no alias behind. The lease folder is deleted
    # directly when Remove-LeaseCredential would refuse, for example over a foreign alias file.
    $inner = $_.Exception
    try { Remove-LeaseCredential -Project $Request.Project -LeaseId $id -SshDir $Request.SshDir | Out-Null }
    catch {
      $dir = Split-Path -Parent $cred.KeyPath
      if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
    }
    throw [InvalidOperationException]::new("The host alias for lease $id could not be set up, and the lease was not placed.", $inner)
  }

  [pscustomobject]@{
    State     = 'ready'
    HostAlias = $set.Alias
    User      = $cred.Principal
    ExpiresAt = $cred.ExpiresUtc
  }
}

function Invoke-Ec2DriverRenew([hashtable]$Request) {
  $c = Update-LeaseCredential -LeaseId $Request.LeaseId -Ttl $Request.Ttl
  [pscustomobject]@{ ExpiresAt = $c.ExpiresUtc }
}

function Invoke-Ec2DriverRelease([hashtable]$Request) {
  $a = @{ Project = $Request.Project; LeaseId = $Request.LeaseId }
  if ($Request.SshDir) { $a.SshDir = $Request.SshDir }
  Remove-LeaseCredential @a
}

Register-LeaseDriver -Name 'ec2' `
  -Place { param([hashtable]$Request) Invoke-Ec2DriverPlace $Request } `
  -Renew { param([hashtable]$Request) Invoke-Ec2DriverRenew $Request } `
  -Release { param([hashtable]$Request) Invoke-Ec2DriverRelease $Request }

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Issues, renews and releases a lease's SSH credential: a key pair signed by the agentspace CA (D7, D17).
# The lease CLI (#0001) dot-sources this file. Its code MUST stay out of scripts\agentspace.ps1 (D18).
# Laptop only: nothing here touches the box or AWS.

$script:LeaseCredentialRepo = Split-Path -Parent $PSScriptRoot
$script:LeaseCredentialKeygen = 'C:\Windows\System32\OpenSSH\ssh-keygen.exe'

. (Join-Path $PSScriptRoot 'lease-alias.ps1')

function Assert-LeaseCredentialCa {
  # The CA is checked by agentspace.ps1 ca (Windows PowerShell 5.1); Assert-Ca MUST NOT be copied (D18).
  $agentspace = Join-Path $script:LeaseCredentialRepo 'scripts\agentspace.ps1'
  $caOut = & powershell -NoProfile -File $agentspace ca 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { throw "The CA check failed: $($caOut.Trim())" }
}

function Invoke-LeaseCredentialSign {
  # Signs the public key at $PubPath; ssh-keygen writes the certificate beside it, as <name>-cert.pub.
  param([string]$PubPath, [string]$LeaseId, [TimeSpan]$Ttl)
  # Laptop state lives in %USERPROFILE%\.agentspace, never under AppData (D21).
  $caKey = Join-Path $env:USERPROFILE '.agentspace\ca\agentspace_ca'
  $cert = $PubPath -replace '\.pub$', '-cert.pub'

  # The validity is an absolute UTC window, start:stop, so it equals the TTL exactly and does not
  # depend on how ssh-keygen reads a relative time.
  $start = [DateTime]::UtcNow
  $start = $start.AddTicks(-($start.Ticks % [TimeSpan]::TicksPerSecond))
  $stop = $start.AddSeconds([Math]::Round($Ttl.TotalSeconds))
  $validity = '{0:yyyyMMddHHmmss}Z:{1:yyyyMMddHHmmss}Z' -f $start, $stop

  $out = & $script:LeaseCredentialKeygen -q -s $caKey -I $LeaseId -n ubuntu -V $validity `
    -O clear -O permit-pty -O permit-port-forwarding $PubPath 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $cert)) { throw "ssh-keygen could not sign ${PubPath}: $out" }
  [pscustomobject]@{ CertificatePath = $cert; ValidFromUtc = $start; ExpiresUtc = $stop }
}

function New-LeaseCredential {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$LeaseId,
    [Parameter(Mandatory)][TimeSpan]$Ttl
  )
  $ErrorActionPreference = 'Stop'

  # The lease ID becomes a folder name and the certificate's key ID, so it MUST be a safe token.
  if ($LeaseId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') {
    throw "Lease ID '$LeaseId' is not a safe token; it MUST match ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}`$."
  }
  if ($Ttl.TotalSeconds -lt 1) { throw 'The TTL MUST be at least one second.' }

  Assert-LeaseCredentialCa

  $dir = Join-Path $env:USERPROFILE ".agentspace\leases\$LeaseId"
  $key = Join-Path $dir 'id_ed25519'
  $cert = "$key-cert.pub"
  # An existing key MUST NOT be overwritten; Update-LeaseCredential renews a lease's certificate.
  if ((Test-Path -LiteralPath $key) -or (Test-Path -LiteralPath $cert)) {
    throw "Lease '$LeaseId' already has a key in $dir; refusing to overwrite it."
  }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null

  # PowerShell 7.3+ passes an empty argument through as '', so -N '' means no passphrase.
  & $script:LeaseCredentialKeygen -q -t ed25519 -N '' -C "agentspace lease $LeaseId" -f $key
  if ($LASTEXITCODE -ne 0) { throw "ssh-keygen could not create $key." }
  & $script:LeaseCredentialKeygen -y -P '' -f $key | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "$key has a passphrase; lease keys MUST NOT have one." }

  $signed = Invoke-LeaseCredentialSign -PubPath "$key.pub" -LeaseId $LeaseId -Ttl $Ttl

  [pscustomobject]@{
    LeaseId         = $LeaseId
    KeyPath         = $key
    CertificatePath = $cert
    Principal       = 'ubuntu'
    ValidFromUtc    = $signed.ValidFromUtc
    ExpiresUtc      = $signed.ExpiresUtc
  }
}

function Update-LeaseCredential {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$LeaseId,
    [Parameter(Mandatory)][TimeSpan]$Ttl
  )
  $ErrorActionPreference = 'Stop'

  # Lease IDs are accepted in any case and emitted in lowercase (D20).
  $LeaseId = $LeaseId.Trim().ToLowerInvariant()
  if ($LeaseId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') {
    throw "Lease ID '$LeaseId' is not a safe token; it MUST match ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}`$."
  }
  if ($Ttl.TotalSeconds -lt 1) { throw 'The TTL MUST be at least one second.' }

  Assert-LeaseCredentialCa

  $dir = Join-Path $env:USERPROFILE ".agentspace\leases\$LeaseId"
  $key = Join-Path $dir 'id_ed25519'
  $cert = "$key-cert.pub"
  foreach ($f in $key, "$key.pub") {
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { throw "Lease '$LeaseId' has no file $f; issue its credential first." }
  }

  # Renewal changes only the certificate, never the key. The new certificate is signed under a
  # temporary name and moved over the old one, so the old one stays until the new one exists.
  $tmpPub = Join-Path $dir "renew-$([guid]::NewGuid().ToString('N')).pub"
  $tmpCert = $tmpPub -replace '\.pub$', '-cert.pub'
  try {
    Copy-Item -LiteralPath "$key.pub" -Destination $tmpPub
    $signed = Invoke-LeaseCredentialSign -PubPath $tmpPub -LeaseId $LeaseId -Ttl $Ttl
    [IO.File]::Move($tmpCert, $cert, $true)
  }
  finally {
    foreach ($f in $tmpPub, $tmpCert) { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force } }
  }

  [pscustomobject]@{
    LeaseId         = $LeaseId
    KeyPath         = $key
    CertificatePath = $cert
    Principal       = 'ubuntu'
    ValidFromUtc    = $signed.ValidFromUtc
    ExpiresUtc      = $signed.ExpiresUtc
  }
}

function Remove-LeaseCredential {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$LeaseId,
    [string]$SshDir,
    [string]$LeaseRoot
  )
  $ErrorActionPreference = 'Stop'
  $id = ConvertTo-LeaseId $LeaseId
  if (-not $LeaseRoot) { $LeaseRoot = Join-Path $env:USERPROFILE '.agentspace\leases' }

  # The alias goes first, and it throws if the file belongs to another lease, so nothing else is
  # deleted then. ~/.ssh/config is never touched: its Include line stays (D9).
  $a = Remove-LeaseAlias -Project $Project -LeaseId $id -SshDir $SshDir

  # Release does not revoke the certificate: it stays valid until it expires (D10). Release needs
  # no CA, so it does not run the CA check.
  $dir = Join-Path $LeaseRoot $id
  $removed = $false
  if (Test-Path -LiteralPath $dir) {
    Remove-Item -LiteralPath $dir -Recurse -Force
    $removed = $true
  }

  [pscustomobject]@{
    LeaseId           = $id
    Alias             = $a.Alias
    AliasPath         = $a.Path
    AliasRemoved      = $a.Removed
    CredentialPath    = $dir
    CredentialRemoved = $removed
  }
}

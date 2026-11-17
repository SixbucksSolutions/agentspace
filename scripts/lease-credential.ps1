#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Issues a lease's SSH credential: a new key pair, signed by the agentspace CA (D7, D17).
# The lease CLI (#0001) dot-sources this file. Its code MUST stay out of scripts\agentspace.ps1 (D18).
# Laptop only: nothing here touches the box or AWS.

$script:LeaseCredentialRepo = Split-Path -Parent $PSScriptRoot
$script:LeaseCredentialKeygen = 'C:\Windows\System32\OpenSSH\ssh-keygen.exe'

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

  # The CA is checked by agentspace.ps1 ca (Windows PowerShell 5.1); Assert-Ca MUST NOT be copied (D18).
  $agentspace = Join-Path $script:LeaseCredentialRepo 'scripts\agentspace.ps1'
  $caOut = & powershell -NoProfile -File $agentspace ca 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { throw "The CA check failed: $($caOut.Trim())" }

  # Laptop state lives in %USERPROFILE%\.agentspace, never under AppData (D21).
  $caDir = Join-Path $env:USERPROFILE '.agentspace\ca'
  $caKey = Join-Path $caDir 'agentspace_ca'

  $dir = Join-Path $env:USERPROFILE ".agentspace\leases\$LeaseId"
  $key = Join-Path $dir 'id_ed25519'
  $cert = "$key-cert.pub"
  # Renewal is a later story (2.07); an existing key MUST NOT be overwritten.
  if ((Test-Path -LiteralPath $key) -or (Test-Path -LiteralPath $cert)) {
    throw "Lease '$LeaseId' already has a key in $dir; refusing to overwrite it."
  }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null

  # PowerShell 7.3+ passes an empty argument through as '', so -N '' means no passphrase.
  & $script:LeaseCredentialKeygen -q -t ed25519 -N '' -C "agentspace lease $LeaseId" -f $key
  if ($LASTEXITCODE -ne 0) { throw "ssh-keygen could not create $key." }
  & $script:LeaseCredentialKeygen -y -P '' -f $key | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "$key has a passphrase; lease keys MUST NOT have one." }

  # The validity is an absolute UTC window, start:stop, so it equals the TTL exactly and does not
  # depend on how ssh-keygen reads a relative time.
  $start = [DateTime]::UtcNow
  $start = $start.AddTicks(-($start.Ticks % [TimeSpan]::TicksPerSecond))
  $stop = $start.AddSeconds([Math]::Round($Ttl.TotalSeconds))
  $validity = '{0:yyyyMMddHHmmss}Z:{1:yyyyMMddHHmmss}Z' -f $start, $stop

  $out = & $script:LeaseCredentialKeygen -q -s $caKey -I $LeaseId -n ubuntu -V $validity `
    -O clear -O permit-pty -O permit-port-forwarding "$key.pub" 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $cert)) { throw "ssh-keygen could not sign ${key}: $out" }

  [pscustomobject]@{
    LeaseId         = $LeaseId
    KeyPath         = $key
    CertificatePath = $cert
    Principal       = 'ubuntu'
    ValidFromUtc    = $start
    ExpiresUtc      = $stop
  }
}

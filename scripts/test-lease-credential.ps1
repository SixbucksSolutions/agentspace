#Requires -Version 7
# Prototype code under D34: the rewrite MUST NOT read this suite. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for scripts\lease-credential.ps1. It issues credentials for throwaway lease IDs,
# inspects them with ssh-keygen, prints PASS or FAIL per check, removes its folders and exits
# non-zero on any failure. Run: pwsh -NoProfile -File scripts\test-lease-credential.ps1
. (Join-Path $PSScriptRoot 'lease-credential.ps1')

$keygen = 'C:\Windows\System32\OpenSSH\ssh-keygen.exe'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}

$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$id = "test-2.05-$stamp"
$leases = Join-Path $env:USERPROFILE '.agentspace\leases'
$ttl = [TimeSpan]::FromMinutes(10)
try {
  $c = New-LeaseCredential -LeaseId $id -Ttl $ttl
  Check 'Issue returns the key and certificate paths' ((Test-Path $c.KeyPath) -and (Test-Path $c.CertificatePath))
  Check 'Returned principal is ubuntu' ($c.Principal -eq 'ubuntu')
  Check 'Returned expiry is start plus TTL' (($c.ExpiresUtc - $c.ValidFromUtc) -eq $ttl)

  $L = (& $keygen -L -f $c.CertificatePath) -join "`n"
  Write-Host $L
  Check 'Type is a user certificate' ($L -match 'Type: ssh-ed25519-cert-v01@openssh.com user certificate')
  Check 'Key ID is the lease ID' ($L -match "(?m)^\s+Key ID: `"$([regex]::Escape($id))`"\s*$")
  Check 'Principals is exactly ubuntu' ($L -match '(?m)^\s+Principals:\s*\r?\n\s+ubuntu\s*\r?\n\s+Critical')
  # ssh-keygen -L prints local time without a zone, so the expected values are converted to local.
  $from = $c.ValidFromUtc.ToLocalTime().ToString('yyyy-MM-ddTHH:mm:ss')
  $to = $c.ExpiresUtc.ToLocalTime().ToString('yyyy-MM-ddTHH:mm:ss')
  # The window is exact only when the certificate's from and to are start and start plus TTL.
  Check 'Valid window is exactly the TTL' (($L -match "Valid: from (\S+) to (\S+)") -and
    ([DateTime]::Parse($Matches[1]) -eq [DateTime]::Parse($from)) -and
    ([DateTime]::Parse($Matches[2]) -eq [DateTime]::Parse($to)) -and
    (([DateTime]::Parse($Matches[2]) - [DateTime]::Parse($Matches[1])) -eq $ttl))
  Check 'Critical Options is none' ($L -match '(?m)^\s+Critical Options: \(none\)')
  Check 'Extensions are exactly permit-pty and permit-port-forwarding' (
    $L -match '(?m)^\s+Extensions:\s*\r?\n\s+permit-port-forwarding\s*\r?\n\s+permit-pty\s*$' -or
    $L -match '(?m)^\s+Extensions:\s*\r?\n\s+permit-pty\s*\r?\n\s+permit-port-forwarding\s*$')

  $caPub = Join-Path $env:USERPROFILE '.agentspace\ca\agentspace_ca.pub'
  $caFp = ((& $keygen -l -f $caPub) -split '\s+')[1]
  Check 'Signing CA is the agentspace CA' ($L -match "Signing CA: \S+ (\S+)" -and $Matches[1] -eq $caFp)

  & $keygen -y -P '' -f $c.KeyPath | Out-Null
  Check 'Key has no passphrase' ($LASTEXITCODE -eq 0)

  $bad = $false
  try { New-LeaseCredential -LeaseId '..\evil' -Ttl $ttl | Out-Null } catch { $bad = $true }
  Check 'Bad lease ID is refused' $bad
  $dup = $false
  try { New-LeaseCredential -LeaseId $id -Ttl $ttl | Out-Null } catch { $dup = $true }
  Check 'Second issue for the same lease ID is refused' $dup
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  Get-ChildItem -LiteralPath $leases -Directory -Filter "test-2.05-$stamp*" -ErrorAction SilentlyContinue |
    Remove-Item -Recurse -Force
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'

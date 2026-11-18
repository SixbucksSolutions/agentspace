#Requires -Version 7
# Prototype code under D34: the rewrite MUST NOT read this suite. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for Update-LeaseCredential and Remove-LeaseCredential in scripts\lease-credential.ps1.
# Renew needs the real CA, so it issues one throwaway lease (a random UUIDv4) under the real leases
# root and deletes exactly that folder at the end. Release works in a throwaway ssh folder under
# $env:TEMP and never touches the real ~/.ssh. It prints PASS or FAIL per check and exits non-zero
# on any failure. Run: pwsh -NoProfile -File scripts\test-lease-renew-release.ps1
. (Join-Path $PSScriptRoot 'lease-credential.ps1')

$keygen = 'C:\Windows\System32\OpenSSH\ssh-keygen.exe'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}

$id = [guid]::NewGuid().ToString()
$missingId = [guid]::NewGuid().ToString()
$leases = Join-Path $env:USERPROFILE '.agentspace\leases'
$leaseDir = Join-Path $leases $id
$missingDir = Join-Path $leases $missingId
$sshTmp = Join-Path $env:TEMP "agentspace-test-ssh-$([guid]::NewGuid().ToString('N'))"
$ttl1 = [TimeSpan]::FromMinutes(10)
$ttl2 = [TimeSpan]::FromMinutes(20)
try {
  $c = New-LeaseCredential -LeaseId $id -Ttl $ttl1
  $keyBytes = [IO.File]::ReadAllBytes($c.KeyPath)
  $fp = (& $keygen -l -f "$($c.KeyPath).pub") -join ' '

  # Renew with the ID in uppercase.
  $r = Update-LeaseCredential -LeaseId $id.ToUpperInvariant() -Ttl $ttl2
  Check 'Renew with an uppercase ID emits the lowercase ID' ($r.LeaseId -eq $id)
  Check 'Renewed expiry is start plus the new TTL' (($r.ExpiresUtc - $r.ValidFromUtc) -eq $ttl2)
  Check 'Renew keeps the private key bytes' ([Linq.Enumerable]::SequenceEqual([byte[]]$keyBytes, [byte[]][IO.File]::ReadAllBytes($r.KeyPath)))
  Check 'Renew keeps the key fingerprint' (((& $keygen -l -f "$($r.KeyPath).pub") -join ' ') -eq $fp)

  $L = (& $keygen -L -f $r.CertificatePath) -join "`n"
  Write-Host $L
  Check 'Key ID is the lowercase lease ID' ($L -match "(?m)^\s+Key ID: `"$([regex]::Escape($id))`"\s*$")
  Check 'Principals is exactly ubuntu' ($L -match '(?m)^\s+Principals:\s*\r?\n\s+ubuntu\s*\r?\n\s+Critical')
  Check 'Critical Options is none' ($L -match '(?m)^\s+Critical Options: \(none\)')
  Check 'Extensions are exactly permit-pty and permit-port-forwarding' (
    $L -match '(?m)^\s+Extensions:\s*\r?\n\s+permit-port-forwarding\s*\r?\n\s+permit-pty\s*$' -or
    $L -match '(?m)^\s+Extensions:\s*\r?\n\s+permit-pty\s*\r?\n\s+permit-port-forwarding\s*$')
  $caPub = Join-Path $env:USERPROFILE '.agentspace\ca\agentspace_ca.pub'
  $caFp = ((& $keygen -l -f $caPub) -split '\s+')[1]
  Check 'Signing CA is the agentspace CA' ($L -match 'Signing CA: \S+ (\S+)' -and $Matches[1] -eq $caFp)
  # ssh-keygen -L prints local time without a zone, so the expected values are converted to local.
  $from = $r.ValidFromUtc.ToLocalTime().ToString('yyyy-MM-ddTHH:mm:ss')
  $to = $r.ExpiresUtc.ToLocalTime().ToString('yyyy-MM-ddTHH:mm:ss')
  Check 'Valid window is exactly the new TTL' (($L -match 'Valid: from (\S+) to (\S+)') -and
    ([DateTime]::Parse($Matches[1]) -eq [DateTime]::Parse($from)) -and
    ([DateTime]::Parse($Matches[2]) -eq [DateTime]::Parse($to)) -and
    (([DateTime]::Parse($Matches[2]) - [DateTime]::Parse($Matches[1])) -eq $ttl2))

  # Renew again: idempotent, key unchanged.
  $r2 = Update-LeaseCredential -LeaseId $id -Ttl $ttl1
  Check 'Second renew succeeds' ($r2.LeaseId -eq $id -and (Test-Path -LiteralPath $r2.CertificatePath))
  Check 'Second renew keeps the key' ([Linq.Enumerable]::SequenceEqual([byte[]]$keyBytes, [byte[]][IO.File]::ReadAllBytes($r2.KeyPath)))
  # The second renew uses the first TTL again, so a renew that left the 20-minute certificate in
  # place fails here.
  $L2 = (& $keygen -L -f $r2.CertificatePath) -join "`n"
  Check 'Second renew writes a certificate with its own window' (($L2 -match 'Valid: from (\S+) to (\S+)') -and
    ([DateTime]::Parse($Matches[1]) -eq $r2.ValidFromUtc.ToLocalTime()) -and
    ([DateTime]::Parse($Matches[2]) -eq $r2.ExpiresUtc.ToLocalTime()) -and
    (([DateTime]::Parse($Matches[2]) - [DateTime]::Parse($Matches[1])) -eq $ttl1))
  Check 'No renew temp files are left' (-not (Get-ChildItem -LiteralPath $leaseDir -Filter 'renew-*'))

  # Renew of a lease with no key.
  $threw = $false
  try { Update-LeaseCredential -LeaseId $missingId -Ttl $ttl1 | Out-Null } catch { $threw = $true }
  Check 'Renew of a lease with no key throws' $threw
  Check 'Renew of a lease with no key creates no folder' (-not (Test-Path -LiteralPath $missingDir))

  # Release, in a throwaway ssh folder.
  New-Item -ItemType Directory -Force -Path $sshTmp | Out-Null
  $set = Set-LeaseAlias -Project 'test' -LeaseId $id -SshDir $sshTmp -LeaseRoot $leases -ProxyCommand 'dummy-proxy'
  $configPath = Join-Path $sshTmp 'config'
  $configBytes = [IO.File]::ReadAllBytes($configPath)
  Check 'Alias file exists before release' (Test-Path -LiteralPath $set.Path)

  $rel = Remove-LeaseCredential -Project 'test' -LeaseId $id.ToUpperInvariant() -SshDir $sshTmp -LeaseRoot $leases
  Check 'Release reports both removed' ($rel.AliasRemoved -and $rel.CredentialRemoved)
  Check 'Release removes the alias file' (-not (Test-Path -LiteralPath $set.Path))
  Check 'Release removes the lease folder' (-not (Test-Path -LiteralPath $leaseDir))
  Check 'Release leaves config byte-identical' ([Linq.Enumerable]::SequenceEqual([byte[]]$configBytes, [byte[]][IO.File]::ReadAllBytes($configPath)))
  Check 'Config still has the Include line' ([Text.Encoding]::ASCII.GetString($configBytes) -match '(?m)^Include agentspace\.d/\*\s*$')

  $rel2 = Remove-LeaseCredential -Project 'test' -LeaseId $id -SshDir $sshTmp -LeaseRoot $leases
  Check 'Second release succeeds with both flags false' (-not $rel2.AliasRemoved -and -not $rel2.CredentialRemoved)
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  if (Test-Path -LiteralPath $leaseDir) { Remove-Item -LiteralPath $leaseDir -Recurse -Force }
  if (Test-Path -LiteralPath $missingDir) { Remove-Item -LiteralPath $missingDir -Recurse -Force }
  if (Test-Path -LiteralPath $sshTmp) { Remove-Item -LiteralPath $sshTmp -Recurse -Force }
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'

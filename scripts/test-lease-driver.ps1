#Requires -Version 7
# Prototype code under D34: the rewrite MUST NOT read this suite. See docs/conformance.md.
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for the driver interface (lease-driver.ps1) and its pooled driver. It places,
# renews and releases one throwaway lease through the interface. The credential step has no root
# override and needs the real CA, so, like test-lease-renew-release.ps1, the lease ID is a random
# UUIDv4 under the real leases root, and exactly that folder is deleted at the end. The ssh folder
# is a throwaway under $env:TEMP, and the real ~/.ssh is hashed before and after to prove it was
# not touched. Nothing connects anywhere. It prints PASS or FAIL per check and exits non-zero on
# any failure. Run: pwsh -NoProfile -File scripts\test-lease-driver.ps1
. (Join-Path $PSScriptRoot 'lease-driver.ps1')

$keygen = 'C:\Windows\System32\OpenSSH\ssh-keygen.exe'
$repo = Split-Path -Parent $PSScriptRoot
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}
function Get-Utc([DateTime]$d) { $d.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }

# A hash of the real ~/.ssh config and alias folder, to prove nothing here touched them.
function Get-RealSshState {
  $ssh = Join-Path $env:USERPROFILE '.ssh'
  $cfg = Join-Path $ssh 'config'
  $c = if (Test-Path -LiteralPath $cfg) { (Get-FileHash -LiteralPath $cfg -Algorithm SHA256).Hash } else { 'absent' }
  $d = Join-Path $ssh 'agentspace.d'
  $names = if (Test-Path -LiteralPath $d) {
    (Get-ChildItem -LiteralPath $d -Force | Sort-Object Name | ForEach-Object { "$($_.Name):$($_.Length):$($_.LastWriteTimeUtc.Ticks)" }) -join '|'
  } else { 'absent' }
  $backups = (Get-ChildItem -LiteralPath $ssh -Filter 'config.agentspace-*' -Force -ErrorAction SilentlyContinue | ForEach-Object Name) -join '|'
  "$c;$names;$backups"
}

function New-LeaseDoc($Id, $Project, $Platform, $Profile, $Size, [int]$TtlSeconds, $Result) {
  $doc = [ordered]@{
    id = $Id; project = $Project; state = $Result.State; platform = $Platform; profile = $Profile
    size = $Size; ttl_seconds = $TtlSeconds; created_at = Get-Utc ([DateTime]::UtcNow)
  }
  if ($Result.State -eq 'ready') {
    $doc.expires_at = Get-Utc $Result.ExpiresAt
    $doc.ssh = [ordered]@{ host_alias = $Result.HostAlias; user = $Result.User }
  }
  else { $doc.reasons = @($Result.Reasons | ForEach-Object { [ordered]@{ code = $_.code; message = $_.message } }) }
  $doc | ConvertTo-Json -Depth 5
}

$schema = Join-Path $repo 'schemas\lease.output.schema.json'
$id = [guid]::NewGuid().ToString()
$id2 = [guid]::NewGuid().ToString()
$id3 = [guid]::NewGuid().ToString()
$leases = Join-Path $env:USERPROFILE '.agentspace\leases'
$leaseDir = Join-Path $leases $id
$sshTmp = Join-Path $env:TEMP "agentspace-test-driver-ssh-$([guid]::NewGuid().ToString('N'))"
$project = 'driver-test'
$ttl1 = [TimeSpan]::FromMinutes(10)
$ttl2 = [TimeSpan]::FromMinutes(20)
$proxy = 'dummy-proxy'
$realBefore = Get-RealSshState
try {
  # Registration: the interface names no driver, and the driver files register themselves.
  $self = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'lease-driver.ps1'))
  Check 'lease-driver.ps1 names no backend' ($self -notmatch '(?i)ec2|aws')
  Check 'A driver registered itself' ($script:LeaseDrivers.Count -ge 1)

  # Place a ready lease.
  $p = Invoke-LeaseDriverPlace -LeaseId $id -Project $project -Platform 'linux/arm64' -Profile 'agent-linux' -Size 'm' `
    -Ttl $ttl1 -SshDir $sshTmp -ProxyCommand $proxy
  $short = $id.Substring(0, 8)
  Check 'Place returns ready' ($p.State -eq 'ready')
  Check 'Place returns a driver name' (-not [string]::IsNullOrEmpty($p.Driver))
  Check 'Place returns the alias agentspace-<project>-<first 8 hex>' ($p.HostAlias -eq "agentspace-$project-$short")
  Check 'Place returns user ubuntu' ($p.User -eq 'ubuntu')
  Check 'ExpiresAt is a UTC whole second' ($p.ExpiresAt.Kind -eq 'Utc' -and $p.ExpiresAt.Ticks % [TimeSpan]::TicksPerSecond -eq 0)
  $cert = Join-Path $leaseDir 'id_ed25519-cert.pub'
  $L = (& $keygen -L -f $cert) -join "`n"
  Check 'ExpiresAt equals the certificate valid-before' (($L -match 'Valid: from (\S+) to (\S+)') -and
    ([DateTime]::Parse($Matches[2]) -eq $p.ExpiresAt.ToLocalTime()))
  Check 'ExpiresAt is now plus the TTL' ([Math]::Abs(($p.ExpiresAt - ([DateTime]::UtcNow + $ttl1)).TotalSeconds) -lt 15)
  $aliasPath = Join-Path $sshTmp "agentspace.d\$($p.HostAlias)"
  Check 'Alias file exists in the throwaway agentspace.d' (Test-Path -LiteralPath $aliasPath)
  $configPath = Join-Path $sshTmp 'config'
  Check 'Config got the Include line' ((Test-Path -LiteralPath $configPath) -and
    ([IO.File]::ReadAllText($configPath) -match '(?m)^Include agentspace\.d/\*\s*$'))
  Check 'Ready lease document validates against the schema' (
    Test-Json -Json (New-LeaseDoc $id $project 'linux/arm64' 'agent-linux' 'm' 600 $p) -SchemaFile $schema -ErrorAction SilentlyContinue)

  # Renew.
  $keyPath = Join-Path $leaseDir 'id_ed25519'
  $keyBytes = [IO.File]::ReadAllBytes($keyPath)
  Start-Sleep -Seconds 2
  $r = Invoke-LeaseDriverRenew -Driver $p.Driver -LeaseId $id -Project $project -Ttl $ttl2
  Check 'Renew changes ExpiresAt' ($r.ExpiresAt -gt $p.ExpiresAt)
  Check 'Renew ExpiresAt is now plus the new TTL' ([Math]::Abs(($r.ExpiresAt - ([DateTime]::UtcNow + $ttl2)).TotalSeconds) -lt 15)
  Check 'Renew keeps the key bytes' ([Linq.Enumerable]::SequenceEqual([byte[]]$keyBytes, [byte[]][IO.File]::ReadAllBytes($keyPath)))
  $L2 = (& $keygen -L -f $cert) -join "`n"
  Check 'Renew ExpiresAt equals the new certificate valid-before' (($L2 -match 'Valid: from (\S+) to (\S+)') -and
    ([DateTime]::Parse($Matches[2]) -eq $r.ExpiresAt.ToLocalTime()))

  # Release, twice.
  Invoke-LeaseDriverRelease -Driver $p.Driver -LeaseId $id -Project $project -SshDir $sshTmp
  Check 'Release removes the alias file' (-not (Test-Path -LiteralPath $aliasPath))
  Check 'Release removes the lease folder' (-not (Test-Path -LiteralPath $leaseDir))
  $threw = $false
  try { Invoke-LeaseDriverRelease -Driver $p.Driver -LeaseId $id -Project $project -SshDir $sshTmp } catch { $threw = $true }
  Check 'A second release succeeds' (-not $threw)

  # Unsatisfiable requests issue nothing.
  $cases = @(
    @{ Name = 'linux/amd64'; Platform = 'linux/amd64'; Profile = 'agent-linux'; Size = 'm'; Codes = @('unsupported_platform') }
    @{ Name = 'agent-linux@1'; Platform = 'linux/arm64'; Profile = 'agent-linux@1'; Size = 'm'; Codes = @('unsupported_profile') }
    @{ Name = 'size l'; Platform = 'linux/arm64'; Profile = 'agent-linux'; Size = 'l'; Codes = @('unsupported_size') }
    @{ Name = 'all three'; Platform = 'linux/amd64'; Profile = 'agent-linux@1'; Size = 'l'
      Codes = @('unsupported_platform', 'unsupported_profile', 'unsupported_size') }
  )
  foreach ($c in $cases) {
    $u = Invoke-LeaseDriverPlace -LeaseId $id2 -Project $project -Platform $c.Platform -Profile $c.Profile -Size $c.Size `
      -Ttl $ttl1 -SshDir $sshTmp -ProxyCommand $proxy
    Check "Unsatisfiable for $($c.Name): state" ($u.State -eq 'unsatisfiable')
    Check "Unsatisfiable for $($c.Name): reasons in order" ((@($u.Reasons.code) -join ',') -eq ($c.Codes -join ','))
    Check "Unsatisfiable for $($c.Name): one sentence naming no backend" (-not (@($u.Reasons) | Where-Object {
          $_.message -match '(?i)ec2|aws|region|instance|box' -or $_.message -notmatch '^[^\r\n]+\.$' }))
    Check "Unsatisfiable for $($c.Name): issues nothing" (
      -not (Test-Path -LiteralPath (Join-Path $leases $id2)) -and
      -not (Test-Path -LiteralPath (Join-Path $sshTmp "agentspace.d\agentspace-$project-$($id2.Substring(0, 8))")))
    Check "Unsatisfiable for $($c.Name): document validates against the schema" (
      Test-Json -Json (New-LeaseDoc $id2 $project $c.Platform $c.Profile $c.Size 600 $u) -SchemaFile $schema -ErrorAction SilentlyContinue)
  }
  $u = Invoke-LeaseDriverPlace -LeaseId $id2 -Project $project -Platform 'linux/amd64' -Profile 'agent-linux@1' -Size 'l' -Ttl $ttl1 -SshDir $sshTmp
  Check 'Unsatisfiable names the value in each message' (
    $u.Reasons[0].message -match 'linux/amd64' -and $u.Reasons[1].message -match 'agent-linux@1' -and $u.Reasons[2].message -match "'l'")

  # A failed alias step leaves no credential behind and keeps the foreign file.
  $aliasDir = Join-Path $sshTmp 'agentspace.d'
  New-Item -ItemType Directory -Force -Path $aliasDir | Out-Null
  $foreign = Join-Path $aliasDir "agentspace-$project-$($id3.Substring(0, 8))"
  [IO.File]::WriteAllText($foreign, "# not an agentspace alias`n")
  $msg = $null
  try { Invoke-LeaseDriverPlace -LeaseId $id3 -Project $project -Platform 'linux/arm64' -Profile 'agent-linux' -Size 'm' -Ttl $ttl1 -SshDir $sshTmp -ProxyCommand $proxy | Out-Null }
  catch { $msg = $_.Exception.Message }
  Check 'A failed alias step throws' ($null -ne $msg)
  Check 'The thrown message names no backend' ($msg -notmatch '(?i)ec2|aws|region|instance')
  Check 'A failed alias step leaves no lease folder' (-not (Test-Path -LiteralPath (Join-Path $leases $id3)))
  Check 'A failed alias step keeps the foreign alias file' ((Test-Path -LiteralPath $foreign) -and
    ([IO.File]::ReadAllText($foreign) -eq "# not an agentspace alias`n"))
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  foreach ($x in $id, $id2, $id3) {
    $d = Join-Path $leases $x
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
  }
  if (Test-Path -LiteralPath $sshTmp) { Remove-Item -LiteralPath $sshTmp -Recurse -Force }
}
Check 'The real ~/.ssh config and agentspace.d are unchanged' ((Get-RealSshState) -eq $realBefore)
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'

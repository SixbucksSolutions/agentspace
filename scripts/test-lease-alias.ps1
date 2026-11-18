#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for scripts\lease-alias.ps1. It works only in a throwaway folder under
# %USERPROFILE%\.agentspace (D21) and MUST NOT read or write the real ~/.ssh. It prints PASS or FAIL
# per check, removes its folder and exits non-zero on any failure.
# Run: pwsh -NoProfile -File scripts\test-lease-alias.ps1
. (Join-Path $PSScriptRoot 'lease-alias.ps1')

$ssh = 'C:\Windows\System32\OpenSSH\ssh.exe'
$failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
  if ($Ok) { Write-Host "PASS: $Name" }
  else { Write-Host "FAIL: $Name $Detail"; $script:failures++ }
}
function Refused([scriptblock]$Block) {
  try { & $Block | Out-Null; $false } catch { $true }
}
function Same([byte[]]$A, [byte[]]$B) { [Linq.Enumerable]::SequenceEqual($A, $B) }
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash }
function Backups([string]$Dir) { @(Get-ChildItem -LiteralPath $Dir -Filter 'config.agentspace-backup-*') }
function Fresh([string]$Name) {
  $d = Join-Path $root $Name
  New-Item -ItemType Directory -Force -Path $d | Out-Null
  $d
}

$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$root = Join-Path $env:USERPROFILE ".agentspace\tmp-test-lease-alias-$stamp"
$idA = '3F2A9C1E-7B4D-4E6A-8C2F-0A1B2C3D4E5F'
$idB = '3f2a9c1e-1111-4222-9333-444455556666'
$proxy = 'C:\Windows\System32\cmd.exe /d /c "echo proxy"'
$enc = [Text.UTF8Encoding]::new($false)
try {
  New-Item -ItemType Directory -Force -Path $root | Out-Null

  # Names
  $alias = Get-LeaseAliasName -Project 'SwimCoach' -LeaseId $idA
  Check 'Mixed-case UUIDv4 and project give a lowercase alias' ($alias -ceq 'agentspace-swimcoach-3f2a9c1e') $alias
  Check 'A non-UUID is refused' (Refused { Get-LeaseAliasName -Project 'x' -LeaseId 'not-a-uuid' })
  Check 'A UUIDv1 is refused' (Refused { Get-LeaseAliasName -Project 'x' -LeaseId '3f2a9c1e-7b4d-1e6a-8c2f-0a1b2c3d4e5f' })
  Check 'A project like ..\x is refused' (Refused { Get-LeaseAliasName -Project '..\x' -LeaseId $idA })

  # Include on an LF config
  $ssh1 = Fresh 'ssh1'
  $cfg1 = Join-Path $ssh1 'config'
  $orig1 = $enc.GetBytes("# My hosts`nHost agentspace`n  User ubuntu`n")
  [IO.File]::WriteAllBytes($cfg1, $orig1)
  $h1 = Hash $cfg1
  $r = Install-LeaseAliasInclude -SshDir $ssh1
  $now = [IO.File]::ReadAllBytes($cfg1)
  $line = $enc.GetBytes("Include agentspace.d/*`n")
  Check 'Include reports a change and a backup' ($r.Changed -and $r.BackupPath -and (Test-Path $r.BackupPath))
  Check 'Include is line 1' ((Same $now[0..($line.Length - 1)] $line))
  Check 'Bytes after the inserted line equal the original' ((Same $now[$line.Length..($now.Length - 1)] $orig1))
  Check 'Backup equals the original' ((Hash $r.BackupPath) -eq $h1)
  Check 'The agentspace.d folder exists' (Test-Path (Join-Path $ssh1 'agentspace.d') -PathType Container)
  $h1b = Hash $cfg1
  $r = Install-LeaseAliasInclude -SshDir $ssh1
  Check 'Second run changes nothing' (-not $r.Changed -and (Hash $cfg1) -eq $h1b -and (Backups $ssh1).Count -eq 1)

  # CRLF config
  $ssh2 = Fresh 'ssh2'
  $cfg2 = Join-Path $ssh2 'config'
  $orig2 = $enc.GetBytes("# CRLF`r`nHost a`r`n  User u`r`n")
  [IO.File]::WriteAllBytes($cfg2, $orig2)
  $r = Install-LeaseAliasInclude -SshDir $ssh2
  $now = [IO.File]::ReadAllBytes($cfg2)
  $line = $enc.GetBytes("Include agentspace.d/*`r`n")
  Check 'CRLF config gets a CRLF Include line, rest byte-identical' ($r.Changed -and (Same $now[0..($line.Length - 1)] $line) -and
    (Same $now[$line.Length..($now.Length - 1)] $orig2))

  # BOM config
  $ssh2b = Fresh 'ssh2b'
  $cfg2b = Join-Path $ssh2b 'config'
  $orig2b = [byte[]](0xEF, 0xBB, 0xBF) + $enc.GetBytes("Host a`n")
  [IO.File]::WriteAllBytes($cfg2b, $orig2b)
  Install-LeaseAliasInclude -SshDir $ssh2b | Out-Null
  $now = [IO.File]::ReadAllBytes($cfg2b)
  Check 'A BOM stays first and the original follows the Include' ($now[0] -eq 0xEF -and $now[1] -eq 0xBB -and $now[2] -eq 0xBF -and
    (Same $now[3..($now.Length - 1)] ($enc.GetBytes("Include agentspace.d/*`n") + $enc.GetBytes("Host a`n"))))

  # Missing config
  $ssh3 = Fresh 'ssh3'
  $r = Install-LeaseAliasInclude -SshDir $ssh3
  Check 'Missing config is created with only the Include line' ($r.Changed -and -not $r.BackupPath -and
    (Same ([IO.File]::ReadAllBytes((Join-Path $ssh3 'config'))) $enc.GetBytes("Include agentspace.d/*`n")))

  # Misplaced Include
  $ssh4 = Fresh 'ssh4'
  $cfg4 = Join-Path $ssh4 'config'
  [IO.File]::WriteAllBytes($cfg4, $enc.GetBytes("Host a`n  User u`nInclude agentspace.d/*`n"))
  $h4 = Hash $cfg4
  Check 'An Include after a Host line is refused' (Refused { Install-LeaseAliasInclude -SshDir $ssh4 })
  Check 'The refused config is unchanged and has no backup' ((Hash $cfg4) -eq $h4 -and (Backups $ssh4).Count -eq 0)

  # Set-LeaseAlias
  $ssh5 = Fresh 'ssh5'
  $leases = Fresh 'leases'
  $leaseDir = Join-Path $leases $idA.ToLowerInvariant()
  New-Item -ItemType Directory -Force -Path $leaseDir | Out-Null
  $key = Join-Path $leaseDir 'id_ed25519'
  Set-Content -LiteralPath $key -Value 'fake key'
  Set-Content -LiteralPath "$key-cert.pub" -Value 'fake cert'
  Check 'Set with a missing certificate is refused' (Refused { Set-LeaseAlias -Project swimcoach -LeaseId $idB -SshDir $ssh5 -LeaseRoot $leases -ProxyCommand $proxy })
  $r = Set-LeaseAlias -Project 'SwimCoach' -LeaseId $idA -SshDir $ssh5 -LeaseRoot $leases -ProxyCommand $proxy
  $file = Join-Path $ssh5 'agentspace.d\agentspace-swimcoach-3f2a9c1e'
  Check 'Set writes the alias file' ($r.Changed -and $r.Alias -ceq 'agentspace-swimcoach-3f2a9c1e' -and $r.Path -eq $file -and (Test-Path $file))
  Check 'Set installed the Include' ((Get-Content -LiteralPath (Join-Path $ssh5 'config') -TotalCount 1) -eq 'Include agentspace.d/*')
  $bytes = [IO.File]::ReadAllBytes($file)
  Check 'Alias file has no BOM' (-not ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB))
  Check 'Alias file has LF line endings only' ($bytes -notcontains 13 -and $bytes[-1] -eq 10)
  $txt = $enc.GetString($bytes)
  Check 'Alias file does not set ClearAllForwardings' ($txt -notmatch '(?i)ClearAllForwardings')
  Check 'Alias file starts with the lease comment' ($txt.StartsWith("# agentspace lease $($idA.ToLowerInvariant()); written by scripts/lease-alias.ps1. Do not edit.`nHost "))
  Check 'Alias folder holds only the alias file' (@(Get-ChildItem (Join-Path $ssh5 'agentspace.d')).Count -eq 1 -and
    @(Get-ChildItem -LiteralPath $ssh5 -Filter '*tmp*').Count -eq 0)

  $G = (& $ssh -G -F $file $r.Alias) -join "`n"
  $keyFwd = $key.Replace('\', '/')
  Check 'ssh -G: user ubuntu' ($G -match '(?m)^user ubuntu\s*$')
  Check 'ssh -G: identityfile names the lease key' ($G -match "(?mi)^identityfile .*$([regex]::Escape($keyFwd))\s*$") $G
  Check 'ssh -G: certificatefile names the lease certificate' ($G -match "(?mi)^certificatefile .*$([regex]::Escape("$keyFwd-cert.pub"))\s*$")
  Check 'ssh -G: identitiesonly yes' ($G -match '(?m)^identitiesonly yes\s*$')
  Check 'ssh -G: hostkeyalias agentspace' ($G -match '(?m)^hostkeyalias agentspace\s*$')
  Check 'ssh -G: stricthostkeychecking true' ($G -match '(?m)^stricthostkeychecking true\s*$')
  Check 'ssh -G: the proxycommand' ($G -match '(?m)^proxycommand C:\\Windows\\System32\\cmd.exe /d /c "echo proxy"\s*$')
  # ssh -G prints each forward's host in brackets.
  Check 'ssh -G: both remoteforward lines' (($G -match '(?m)^remoteforward \[127\.0\.0\.1\]:8811 \[127\.0\.0\.1\]:8811\s*$') -and
    ($G -match '(?m)^remoteforward \[127\.0\.0\.1\]:53147 \[127\.0\.0\.1\]:53147\s*$'))

  # The default ProxyCommand names the alias literally at the end, and never uses %n.
  # The repo root is pointed at a throwaway folder that has an .instance-id, so no real checkout is read.
  $fakeRepo = Fresh 'fakerepo'
  Set-Content -LiteralPath (Join-Path $fakeRepo '.instance-id') -Value 'i-fake'
  $script:LeaseAliasRepo = $fakeRepo
  $ssh6 = Fresh 'ssh6'
  $r6 = Set-LeaseAlias -Project 'SwimCoach' -LeaseId $idA -SshDir $ssh6 -LeaseRoot $leases
  $txt6 = [IO.File]::ReadAllText($r6.Path)
  $expectPc = 'ProxyCommand C:\Windows\System32\cmd.exe /d /c "' + (Join-Path $script:LeaseAliasRepo 'scripts\ssh-proxy.cmd') + '" agentspace-swimcoach-3f2a9c1e'
  Check 'Default ProxyCommand line ends with the literal alias' ($txt6.Contains("`n  $expectPc`n")) $txt6
  Check 'Default ProxyCommand has no %n' ($txt6 -notmatch '%n')
  $G6 = (& $ssh -G -F $r6.Path $r6.Alias) -join "`n"
  Check 'ssh -G: default proxycommand ends with the alias' ($G6 -match '(?m)^proxycommand .*ssh-proxy\.cmd" agentspace-swimcoach-3f2a9c1e\s*$') $G6
  Check 'A custom -ProxyCommand is written unchanged' ($txt -match '(?m)^  ProxyCommand C:\\Windows\\System32\\cmd\.exe /d /c "echo proxy"$')

  # Set again
  $t0 = (Get-Item -LiteralPath $file).LastWriteTimeUtc
  Start-Sleep -Milliseconds 1100
  $r2 = Set-LeaseAlias -Project 'swimcoach' -LeaseId $idA -SshDir $ssh5 -LeaseRoot $leases -ProxyCommand $proxy
  Check 'Set again changes nothing' (-not $r2.Changed -and (Same ([IO.File]::ReadAllBytes($file)) $bytes) -and
    (Get-Item -LiteralPath $file).LastWriteTimeUtc -eq $t0)

  # Collision: same project and first 8 hex, different lease
  $dirB = Join-Path $leases $idB
  New-Item -ItemType Directory -Force -Path $dirB | Out-Null
  Set-Content -LiteralPath (Join-Path $dirB 'id_ed25519') -Value 'fake key'
  Set-Content -LiteralPath (Join-Path $dirB 'id_ed25519-cert.pub') -Value 'fake cert'
  Check 'A short-ID collision is refused' (Refused { Set-LeaseAlias -Project swimcoach -LeaseId $idB -SshDir $ssh5 -LeaseRoot $leases -ProxyCommand $proxy })
  Check 'The existing alias file is unchanged after a collision' ((Same ([IO.File]::ReadAllBytes($file)) $bytes))
  Check 'Removing the colliding lease is refused' (Refused { Remove-LeaseAlias -Project swimcoach -LeaseId $idB -SshDir $ssh5 })
  Check 'The existing alias file survives that removal' ((Same ([IO.File]::ReadAllBytes($file)) $bytes))

  # Remove
  $cfgHash = Hash (Join-Path $ssh5 'config')
  $r = Remove-LeaseAlias -Project 'swimcoach' -LeaseId $idA -SshDir $ssh5
  Check 'Remove deletes the alias file' ($r.Removed -and -not (Test-Path $file))
  $r = Remove-LeaseAlias -Project 'swimcoach' -LeaseId $idA -SshDir $ssh5
  Check 'Remove again reports Removed false without error' (-not $r.Removed)
  Check 'Remove leaves config and the lease folder alone' ((Hash (Join-Path $ssh5 'config')) -eq $cfgHash -and (Test-Path $key) -and (Test-Path "$key-cert.pub"))
}
catch { Check 'No unexpected error' $false $_.Exception.Message }
finally {
  if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
if ($failures) { Write-Host "$failures check(s) failed."; exit 1 }
Write-Host 'All checks passed.'

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Writes and removes a lease's host alias (D9, D21): one file per lease in ~/.ssh/agentspace.d,
# pulled in by a single `Include agentspace.d/*` line at the top of ~/.ssh/config.
# The lease CLI (#0001) dot-sources this file. Its code MUST stay out of scripts\agentspace.ps1 (D18).
# Laptop only: nothing here touches the box or AWS.
#
# ~/.ssh/config is Terry's own file. It MUST stay byte-identical outside the one inserted line,
# so it is edited as bytes and never through a parsed or re-joined text form. Nothing here adds a BOM;
# a BOM the config already has is kept as it was.

$script:LeaseAliasRepo = Split-Path -Parent $PSScriptRoot
$script:LeaseAliasIncludeLine = 'Include agentspace.d/*'

function ConvertTo-LeaseId {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$LeaseId)
  # Lease IDs are UUIDv4, accepted in any case and emitted in lowercase (D20).
  $id = $LeaseId.Trim().ToLowerInvariant()
  if ($id -notmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$') {
    throw "Lease ID '$LeaseId' is not a UUIDv4."
  }
  $id
}

function ConvertTo-LeaseProject {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$Project)
  $p = $Project.Trim().ToLowerInvariant()
  if ($p -notmatch '^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$') {
    throw "Project '$Project' MUST match ^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?`$ once lowercased."
  }
  $p
}

function Get-LeaseAliasName {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$LeaseId
  )
  $p = ConvertTo-LeaseProject $Project
  $id = ConvertTo-LeaseId $LeaseId
  "agentspace-$p-$($id.Substring(0, 8))"
}

function Get-LeaseAliasSshDir([string]$SshDir) {
  if ($SshDir) { $SshDir } else { Join-Path $env:USERPROFILE '.ssh' }
}

function Install-LeaseAliasInclude {
  [CmdletBinding()]
  param([string]$SshDir)
  $ErrorActionPreference = 'Stop'
  $SshDir = Get-LeaseAliasSshDir $SshDir
  $config = Join-Path $SshDir 'config'
  $includeBytes = [Text.Encoding]::ASCII.GetBytes($script:LeaseAliasIncludeLine)

  if (-not (Test-Path -LiteralPath $config -PathType Leaf)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $SshDir 'agentspace.d') | Out-Null
    $bytes = $includeBytes + [byte]10
    [IO.File]::WriteAllBytes($config, $bytes)
    return [pscustomobject]@{ ConfigPath = $config; Changed = $true; BackupPath = $null }
  }

  $orig = [IO.File]::ReadAllBytes($config)
  $bom = ($orig.Length -ge 3 -and $orig[0] -eq 0xEF -and $orig[1] -eq 0xBB -and $orig[2] -eq 0xBF)
  $skip = if ($bom) { 3 } else { 0 }

  # Latin-1 maps each byte to one character, so this text is only for finding lines, never written back.
  $text = [Text.Encoding]::Latin1.GetString($orig, $skip, $orig.Length - $skip)
  $seenBlock = $null
  $found = $false
  $misplaced = $false
  foreach ($line in $text -split "`n") {
    $t = $line.Trim()
    if ($t -match '^Include\s+agentspace\.d/\*$') {
      if ($null -eq $seenBlock) { $found = $true; break }
      $misplaced = $true
    }
    elseif ($null -eq $seenBlock -and $t -match '^(Host|Match)(\s|=|$)') { $seenBlock = $t }
  }
  if ($misplaced -and -not $found) {
    throw "$config has '$($script:LeaseAliasIncludeLine)' after the line '$seenBlock', so it belongs to that block. A person MUST move it to the top of the file; nothing was changed."
  }
  New-Item -ItemType Directory -Force -Path (Join-Path $SshDir 'agentspace.d') | Out-Null
  if ($found) {
    return [pscustomobject]@{ ConfigPath = $config; Changed = $false; BackupPath = $null }
  }

  # Back up first, and prove the copy holds the bytes read above, so an edit saved in between is
  # never silently dropped.
  $stamp = [DateTime]::UtcNow.ToString("yyyyMMdd'T'HHmmss'Z'")
  $backup = Join-Path $SshDir "config.agentspace-backup-$stamp"
  [IO.File]::Copy($config, $backup, $false)
  $origHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($orig))
  if ((Get-FileHash -LiteralPath $backup -Algorithm SHA256).Hash -ne $origHash) {
    throw "$config changed while it was read, or the backup $backup does not match it; nothing was changed."
  }

  [byte[]]$eol = if ($text.Contains("`r`n")) { 13, 10 } else { , 10 }
  $new = [Collections.Generic.List[byte]]::new()
  if ($bom) { $new.AddRange([byte[]]($orig[0..2])) }
  $new.AddRange($includeBytes)
  $new.AddRange($eol)
  if ($orig.Length -gt $skip) { $new.AddRange([byte[]]($orig[$skip..($orig.Length - 1)])) }

  # Replace keeps the original file's ACL, which Windows OpenSSH checks.
  $tmp = Join-Path $SshDir "config.agentspace-tmp-$([guid]::NewGuid().ToString('N'))"
  try {
    [IO.File]::WriteAllBytes($tmp, $new.ToArray())
    [IO.File]::Replace($tmp, $config, [NullString]::Value)
  }
  finally {
    # If Replace failed partway and the config is gone, the temp file is the only new copy: keep it.
    if ((Test-Path -LiteralPath $tmp) -and (Test-Path -LiteralPath $config)) { Remove-Item -LiteralPath $tmp -Force }
  }
  [pscustomobject]@{ ConfigPath = $config; Changed = $true; BackupPath = $backup }
}

function Set-LeaseAlias {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$LeaseId,
    [string]$SshDir,
    [string]$LeaseRoot,
    [string]$ProxyCommand
  )
  $ErrorActionPreference = 'Stop'
  $alias = Get-LeaseAliasName -Project $Project -LeaseId $LeaseId
  $id = ConvertTo-LeaseId $LeaseId
  $SshDir = Get-LeaseAliasSshDir $SshDir
  if (-not $LeaseRoot) { $LeaseRoot = Join-Path $env:USERPROFILE '.agentspace\leases' }

  $key = Join-Path (Join-Path $LeaseRoot $id) 'id_ed25519'
  $cert = "$key-cert.pub"
  foreach ($f in $key, $cert) {
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { throw "Lease $id has no file $f; issue its credential first." }
  }

  if (-not $ProxyCommand) {
    # ssh-proxy.cmd reads .instance-id from its own checkout, and a worktree has none.
    if (-not (Test-Path -LiteralPath (Join-Path $script:LeaseAliasRepo '.instance-id'))) {
      throw "$($script:LeaseAliasRepo) has no .instance-id, so its ssh-proxy.cmd cannot reach the box. Run from the main checkout or pass -ProxyCommand."
    }
    # The alias is written out literally, never as ssh's %n: the Claude desktop app runs this line through
    # its own proxy (D12), and nothing shows that it expands tokens. ssh-proxy.cmd passes it to `up -Alias`,
    # so the token push rides this lease's own key and certificate.
    $ProxyCommand = 'C:\Windows\System32\cmd.exe /d /c "' + (Join-Path $script:LeaseAliasRepo 'scripts\ssh-proxy.cmd') + '" ' + $alias
  }

  # The alias is on a command line, so it MUST stay within what `up -Alias` accepts.
  if ($alias -notmatch '^[A-Za-z0-9._-]+$') { throw "Alias '$alias' MUST match ^[A-Za-z0-9._-]+`$." }

  # A line break would add directives to the alias; ssh expands % tokens, so a literal % is %%.
  foreach ($v in $key, $ProxyCommand) {
    if ($v -match '[\r\n]') { throw "A path or ProxyCommand for alias $alias contains a line break; refusing it." }
  }

  Install-LeaseAliasInclude -SshDir $SshDir | Out-Null

  $keyFwd = $key.Replace('\', '/').Replace('%', '%%')
  $certFwd = $cert.Replace('\', '/').Replace('%', '%%')
  $ProxyCommand = $ProxyCommand.Replace('%', '%%')
  $lines = @(
    "# agentspace lease $id; written by scripts/lease-alias.ps1. Do not edit."
    "Host $alias"
    '  User ubuntu'
    "  IdentityFile `"$keyFwd`""
    "  CertificateFile `"$certFwd`""
    '  IdentitiesOnly yes'
    '  HostKeyAlias agentspace'
    '  StrictHostKeyChecking yes'
    "  ProxyCommand $ProxyCommand"
    '  ServerAliveInterval 30'
    '  ServerAliveCountMax 4'
    '  RemoteForward 127.0.0.1:8811 127.0.0.1:8811'
    '  RemoteForward 127.0.0.1:53147 127.0.0.1:53147'
  )
  # UTF-8 without a BOM; it equals ASCII unless a path has other characters.
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($lines -join "`n") + "`n")

  $dir = Join-Path $SshDir 'agentspace.d'
  $path = Join-Path $dir $alias
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    $old = [IO.File]::ReadAllBytes($path)
    $first = ([Text.Encoding]::Latin1.GetString($old) -split "`n")[0]
    if ($first -notmatch '^# agentspace lease (\S+);') { throw "$path exists and is not an agentspace alias file; refusing to replace it." }
    if ($Matches[1] -ne $id) { throw "$path belongs to lease $($Matches[1]); alias $alias collides with lease $id." }
    if ([Linq.Enumerable]::SequenceEqual([byte[]]$old, [byte[]]$bytes)) {
      return [pscustomobject]@{ Alias = $alias; Path = $path; Changed = $false }
    }
  }

  # The temp file stays out of agentspace.d, because the Include glob would read it there.
  $tmp = Join-Path $SshDir "$alias.agentspace-tmp-$([guid]::NewGuid().ToString('N'))"
  try {
    [IO.File]::WriteAllBytes($tmp, $bytes)
    [IO.File]::Move($tmp, $path, $true)
  }
  finally { if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force } }
  [pscustomobject]@{ Alias = $alias; Path = $path; Changed = $true }
}

function Remove-LeaseAlias {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Project,
    [Parameter(Mandatory)][string]$LeaseId,
    [string]$SshDir
  )
  $ErrorActionPreference = 'Stop'
  $alias = Get-LeaseAliasName -Project $Project -LeaseId $LeaseId
  $id = ConvertTo-LeaseId $LeaseId
  $SshDir = Get-LeaseAliasSshDir $SshDir
  $path = Join-Path (Join-Path $SshDir 'agentspace.d') $alias
  $removed = $false
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    # Another lease's file at this alias (a short-ID collision, or a foreign file) MUST NOT be deleted.
    $first = ([Text.Encoding]::Latin1.GetString([IO.File]::ReadAllBytes($path)) -split "`n")[0]
    if ($first -notmatch '^# agentspace lease (\S+);' -or $Matches[1] -ne $id) {
      throw "$path does not belong to lease $id; refusing to remove it."
    }
    Remove-Item -LiteralPath $path -Force
    $removed = $true
  }
  [pscustomobject]@{ Alias = $alias; Path = $path; Removed = $removed }
}

# The live kit's mask (docs/architect.md, "Live checks on Terry's laptop and the box"). Process tool,
# not product (D28).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# Usage, dot-sourced:
#   . scripts\live\mask.ps1
#   ConvertTo-LiveMasked -Text $text          # one string, lines and all
#   Get-Content -Raw <file> | ConvertTo-LiveMasked
#
# It is v0.08-T05's mask (docs/runs/e0.08-t05-s01.md, "The mask, and its proof"): the same 18 rules in
# the same order, with v0.08-T06's 8.3 short form of the profile path and folder added to rules 1 and 2.
# A live run on Terry's real setup MUST pass every byte it prints or writes through it (AGENTS.md), and
# MUST run scripts\live\mask-proof.ps1 first.
#
# Dot-sourcing defines functions and the variables named LiveMask*, and nothing else: it MUST NOT set
# a common name such as $Path in the caller's scope (v0.08-T06: a scan's $Path was reset that way, and
# the scan passed every file). Every regex that names an AWS identifier's prefix is assembled from
# pieces, so this file passes scripts\scan-identifiers.ps1.

Set-StrictMode -Version Latest

# The 8.3 short form of a folder, or the folder itself when the volume keeps no short names.
function Get-LiveMaskShortPath([string]$LongPath) {
  try {
    $fso = New-Object -ComObject Scripting.FileSystemObject
    return [string]$fso.GetFolder($LongPath).ShortPath
  }
  catch {
    return $LongPath
  }
}

# A path's prefix, matched in either slash direction, with doubled backslashes, and in Git Bash's
# /c/... form.
function Get-LiveMaskPathPattern([string]$FullPath) {
  $parts = @($FullPath -split '[\\/]+' | Where-Object { $_ -ne '' })
  if ($parts.Count -eq 0) { return $null }
  $sep = '(?:\\\\|\\|/)'
  $escaped = @($parts | ForEach-Object { [regex]::Escape($_) })
  $windows = $escaped -join $sep
  $drive = $parts[0].TrimEnd(':')
  $bash = $null
  if ($parts[0] -match '^[A-Za-z]:$' -and $parts.Count -gt 1) {
    $bash = '/' + [regex]::Escape($drive) + '/' + (($escaped | Select-Object -Skip 1) -join '/')
  }
  if ($bash) { return "(?:$windows|$bash)" }
  return $windows
}

$LiveMaskProfile = [string]$env:USERPROFILE
$LiveMaskProfileShort = if ($LiveMaskProfile) { Get-LiveMaskShortPath $LiveMaskProfile } else { '' }

# ConvertTo-LiveMasked: the 18 rules, in order. -Profiles and -Literals default to the real
# environment; mask-proof.ps1 passes made-up ones to prove rules 1 and 2 on planted values.
function ConvertTo-LiveMasked {
  [CmdletBinding()]
  param(
    [Parameter(ValueFromPipeline = $true)][AllowEmptyString()][AllowNull()][string]$Text,
    [string[]]$Profiles,
    [string[]]$Literals
  )
  process {
    if ($null -eq $Text) { return '' }
    $t = $Text
    if (-not $PSBoundParameters.ContainsKey('Profiles')) {
      $Profiles = @($LiveMaskProfile, $LiveMaskProfileShort)
    }
    if (-not $PSBoundParameters.ContainsKey('Literals')) {
      $Literals = @($env:COMPUTERNAME, $env:USERNAME)
      foreach ($p in @($LiveMaskProfile, $LiveMaskProfileShort)) {
        if ($p) { $Literals += (Split-Path -Leaf $p) }
      }
    }

    # 1. The profile path's prefix, long and 8.3 short, longest first.
    foreach ($p in @($Profiles | Where-Object { $_ } | Select-Object -Unique | Sort-Object -Property Length -Descending)) {
      $pat = Get-LiveMaskPathPattern $p
      if ($pat) { $t = [regex]::Replace($t, $pat, '%USERPROFILE%', 'IgnoreCase') }
    }
    # 2. The computer name, the user name and the profile's last folder, long and short.
    foreach ($l in @($Literals | Where-Object { $_ } | Select-Object -Unique | Sort-Object -Property Length -Descending)) {
      $t = [regex]::Replace($t, [regex]::Escape($l), '<literal>', 'IgnoreCase')
    }
    # 3. PEM private key blocks, whole, then any lone marker line.
    $pem = '-----BEGIN [A-Z ]*' + 'PRIVATE KEY-----'
    $pemEnd = '-----END [A-Z ]*' + 'PRIVATE KEY-----'
    $t = [regex]::Replace($t, "$pem[\s\S]*?$pemEnd", '<private-key>')
    $t = [regex]::Replace($t, "$pem|$pemEnd", '<private-key>')
    # 4. The values of StackId and ChangeSetId keys.
    $t = [regex]::Replace($t, '("?(?:StackId|ChangeSetId)"?\s*[:=]\s*"?)[^"\s,}]+', '$1<stack-id>')
    # 5. ARNs.
    $t = [regex]::Replace($t, ('ar' + 'n:aws') + '[A-Za-z-]*:[^\s"'',]*', '<arn>')
    # 6. SSH certificates, then 7. SSH public keys with any comment on the same line.
    $keyTypes = '(?:ssh-ed25519|ssh-rsa|ssh-dss|ecdsa-sha2-nistp\d+|sk-ssh-ed25519@openssh\.com)'
    $t = [regex]::Replace($t, '(?:ssh-ed25519|ssh-rsa|ssh-dss|ecdsa-sha2-nistp\d+)-cert-v01@openssh\.com[ \t]+\S+([ \t]+\S+)?', '<ssh-cert>')
    $t = [regex]::Replace($t, $keyTypes + '[ \t]+[A-Za-z0-9+/=]{16,}([ \t]+\S+)?', '<ssh-key>')
    # 8. SHA256 key fingerprints.
    $t = [regex]::Replace($t, 'SHA256:[A-Za-z0-9+/=]{20,}', '<fingerprint>')
    # 9. Token, secret and password assignments.
    $t = [regex]::Replace($t, '(?i)(\b[A-Za-z0-9_-]*(?:token|secret|password|passwd|api_?key)[A-Za-z0-9_-]*"?\s*[:=]\s*"?)[^\s"'',}]+', '$1<secret>')
    # 10. Email addresses.
    $t = [regex]::Replace($t, '[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}', '<email>')
    # 11. UUIDs.
    $t = [regex]::Replace($t, '\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b', '<uuid>')
    # 12. Lease aliases.
    $t = [regex]::Replace($t, '\bagentspace-[a-z0-9][a-z0-9-]*-[0-9a-f]{8}\b', '<lease-alias>')
    # 13. Public EC2 host names, then 14. private ones.
    $t = [regex]::Replace($t, '\bec2-\d{1,3}-\d{1,3}-\d{1,3}-\d{1,3}(?:\.[A-Za-z0-9-]+)*\.amazonaws\.com\b', '<public-host>')
    $t = [regex]::Replace($t, '\b' + 'ip' + '-\d{1,3}-\d{1,3}-\d{1,3}-\d{1,3}(?:\.[A-Za-z0-9-]+)*', '<private-host>')
    # 15. IPv4 addresses: private ranges as <private-ip>, loopback kept, every other as <public-ip>.
    $t = [regex]::Replace($t, '(?<![\d.])(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?![\d.]*\d)', {
        param($m)
        $o = @(1..4 | ForEach-Object { [int]$m.Groups[$_].Value })
        if (@($o | Where-Object { $_ -gt 255 }).Count) { return $m.Value }
        if ($o[0] -eq 127) { return $m.Value }
        if ($o[0] -eq 10 -or ($o[0] -eq 172 -and $o[1] -ge 16 -and $o[1] -le 31) -or ($o[0] -eq 192 -and $o[1] -eq 168)) {
          return '<private-ip>'
        }
        return '<public-ip>'
      })
    # 16. Instance IDs, and the other EC2 resource IDs.
    $t = [regex]::Replace($t, '\b' + 'i' + '-(?:[0-9a-f]{17}|[0-9a-f]{8})\b', '<instance-id>')
    $t = [regex]::Replace($t, '\b(vpc|subnet|vol|sg|ami|eni|igw|rtb|snap|r)' + '-(?:[0-9a-f]{17}|[0-9a-f]{8})\b', '<$1-id>')
    # 17. Runs of 32 or more token characters holding a digit, an uppercase and a lowercase letter.
    $t = [regex]::Replace($t, '[A-Za-z0-9_+/=-]{32,}', {
        param($m)
        $v = $m.Value
        if ($v -cmatch '\d' -and $v -cmatch '[A-Z]' -and $v -cmatch '[a-z]') { return '<token>' }
        return $v
      })
    # 18. Last, any run of exactly 12 digits.
    $t = [regex]::Replace($t, '(?<!\d)\d{12}(?!\d)', '<account-id>')
    return $t
  }
}

# A file's text, masked. A missing file gives one masked line saying so.
function Get-LiveMaskedFile([string]$LiteralFile) {
  if (-not (Test-Path -LiteralPath $LiteralFile)) {
    return (ConvertTo-LiveMasked -Text "Missing: $LiteralFile")
  }
  return (ConvertTo-LiveMasked -Text ([IO.File]::ReadAllText($LiteralFile)))
}

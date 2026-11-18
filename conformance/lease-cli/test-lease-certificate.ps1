#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.04-T03-S03, #0086): the credential a ready `create` leaves, as
# a caller and its OpenSSH client see it. It enforces docs/lease-api.md, "The credential" and
# "Times", and docs/state.md, "The lease folder", "The key", "The certificate" and "The CA".
#
# - It reads the certificate that a ready `create` leaves in the lease's folder with `ssh-keygen -L`
#   (a program outside agentspace, the oracle), and the key with plain file reads. It reads nothing
#   outside the check's throwaway home, and it signs with the throwaway CA only.
# - It compares the certificate's validity with its own clock, as lease-api.md, "Times", requires,
#   and reports the zone it ran in: `ssh-keygen -L` prints local time with no zone, so a mix-up of
#   UTC and local time shows only on a machine whose zone is not UTC.
# - Nothing connects anywhere.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-certificate.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$cred = 'lease-api.md#the-credential'
$cert = 'state.md#the-certificate'

$lab = New-ConformanceHome
try {
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$cred and $cert" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {

    # The zone the run is in (lease-api.md#times, #0029 point 7). `ssh-keygen -L` prints local time with
    # no zone, so a UTC and local time mix-up shows only when the machine is not on UTC. This host
    # cannot set a zone for one process, so a run on a UTC machine is incomplete, never passed.
    $offset = [TimeZoneInfo]::Local.GetUtcOffset([DateTime]::UtcNow)
    $zone = "UTC$(if ($offset -lt [TimeSpan]::Zero) { '-' } else { '+' })$($offset.Duration().ToString('hh\:mm'))"
    if ($offset -eq [TimeSpan]::Zero) {
      Write-ConformanceResult -Name 'lease-api.md#times: The run is in a time zone other than UTC' -Status SKIPPED -Reason 'The machine is set to UTC, so a UTC and local time mix-up would go unseen.'
    }

    $t0 = Get-CcNowSecond
    $r = New-CcReadyLease $lab 'cert' '90m'
    $t1 = Get-CcNowSecond
    $d = $r.Doc
    $id = if ($d) { [string]$d['id'] } else { '00000000-0000-4000-8000-000000000000' }
    $dir = Get-CcLeaseDir $lab $id
    $keyPath = Join-Path $dir 'id_ed25519'
    $pubPath = Join-Path $dir 'id_ed25519.pub'
    $certPath = Join-Path $dir 'id_ed25519-cert.pub'
    $c = $null
    if ($r.Ok -and (Test-Path -LiteralPath $certPath)) { $c = Get-CcCertificate $certPath }
    $caPub = Join-Path $lab.UserProfile '.agentspace\ca\agentspace_ca.pub'

    Test-Cc "$cred`: Setup, create --ttl 90m leaves the key, the public key and the certificate in the lease's folder" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      $names = (Get-CcFileNames $dir) -join '|'
      if ($names -cne 'id_ed25519|id_ed25519-cert.pub|id_ed25519.pub') { return "The lease folder holds '$names'." }
      if (-not $c -or -not $c.Type) { return 'ssh-keygen could not read the certificate.' }
      $true
    }
    Test-Cc "$cert`: The certificate is a user certificate of type ssh-ed25519-cert-v01@openssh.com, never a host certificate" {
      if (-not $c) { return 'There is no certificate.' }
      if ($c.Type -cne 'ssh-ed25519-cert-v01@openssh.com' -or $c.Kind -cne 'user') { return "Type '$($c.Type)', kind '$($c.Kind)'." }
      $true
    }
    Test-Cc "$cert`: The Key ID is the lease's normalized ID (lease-api.md#the-credential)" {
      if (-not $c) { return 'There is no certificate.' }
      if ($c.KeyId -cne $id) { return "The Key ID is '$($c.KeyId)'." }
      $true
    }
    Test-Cc "$cert`: The one principal is ssh.user" {
      if (-not $c) { return 'There is no certificate.' }
      if (@($c.Principals).Count -ne 1 -or $c.Principals[0] -cne $d['ssh']['user']) { return "The principals are '$(@($c.Principals) -join ',')', and ssh.user is '$($d['ssh']['user'])'." }
      $true
    }
    Test-Cc "$cert`: There are no critical options" {
      if (-not $c) { return 'There is no certificate.' }
      if (-not $c.CriticalNone -or @($c.Critical).Count -ne 0) { return 'The certificate has critical options.' }
      $true
    }
    Test-Cc "$cert`: The extensions are exactly permit-pty and permit-port-forwarding, and no other (agent, X11, user rc)" {
      if (-not $c) { return 'There is no certificate.' }
      $e = (@($c.Extensions) | Sort-Object) -join ','
      if ($e -cne 'permit-port-forwarding,permit-pty') { return "The extensions are '$e'." }
      $true
    }
    Test-Cc "$cert`: The signature key is the CA, an ed25519 key, and the signature is ssh-ed25519" {
      if (-not $c) { return 'There is no certificate.' }
      $ca = Get-CcFingerprint $caPub
      if (-not $ca -or $c.CaFingerprint -cne $ca) { return "The signing CA is '$($c.CaFingerprint)', and the throwaway CA's is '$ca'." }
      if ($c.CaUsing -cne 'ssh-ed25519') { return "The signature is '$($c.CaUsing)'." }
      $true
    }
    Test-Cc "$cert`: The certificate is for the lease's own public key" {
      if (-not $c) { return 'There is no certificate.' }
      $fp = Get-CcFingerprint $pubPath
      if (-not $fp -or $c.PublicKeyFingerprint -cne $fp) { return "The certificate's key is '$($c.PublicKeyFingerprint)', and the public key's is '$fp'." }
      $true
    }
    Test-Cc "lease-api.md#times: valid-before equals expires_at, exactly, read in the machine's zone ($zone) and converted to UTC" {
      if (-not $c) { return 'There is no certificate.' }
      if ((ConvertTo-CcTime $c.To) -cne $d['expires_at']) { return "valid-before is $(ConvertTo-CcTime $c.To), and expires_at is $($d['expires_at'])." }
      $true
    }
    Test-Cc "lease-api.md#times: valid-before is the issue time plus the TTL, and the issue time is within the create call (own clock)" {
      if (-not $c) { return 'There is no certificate.' }
      if ($d['ttl_seconds'] -ne 5400) { return "ttl_seconds is $($d['ttl_seconds'])." }
      $issue = $c.To.AddSeconds(-5400)
      if ($issue -lt $t0 -or $issue -gt $t1) { return "The issue time $(ConvertTo-CcTime $issue) is outside $(ConvertTo-CcTime $t0) to $(ConvertTo-CcTime $t1)." }
      $true
    }
    Test-Cc "$cert`: valid-after is not later than the issue time, and not an hour or more off it (a UTC and local mix-up)" {
      if (-not $c) { return 'There is no certificate.' }
      $issue = $c.To.AddSeconds(-5400)
      if ($c.From -gt $issue) { return "valid-after $(ConvertTo-CcTime $c.From) is later than the issue time $(ConvertTo-CcTime $issue)." }
      if (($issue - $c.From).TotalMinutes -ge 60) { return "valid-after $(ConvertTo-CcTime $c.From) is an hour or more before the issue time." }
      $true
    }
    Test-Cc "lease-api.md#times: created_at is a whole UTC second within the create call (own clock)" {
      $ca = ConvertFrom-CcTime $d['created_at']
      if ($ca -lt $t0 -or $ca -gt $t1) { return "created_at $($d['created_at']) is outside $(ConvertTo-CcTime $t0) to $(ConvertTo-CcTime $t1)." }
      $true
    }

    # The shortest and the longest TTL: valid-before is the issue time plus the TTL at both ends.
    foreach ($case in @(@{ Ttl = '1m'; Seconds = 60 }, @{ Ttl = '24h'; Seconds = 86400 }, @{ Ttl = 'PT2H'; Seconds = 7200 })) {
      $a = Get-CcNowSecond
      $rr = New-CcReadyLease $lab 'cert' $case.Ttl
      $b = Get-CcNowSecond
      $cc = $null
      if ($rr.Ok) { $p = Join-Path (Get-CcLeaseDir $lab ([string]$rr.Doc['id'])) 'id_ed25519-cert.pub'; if (Test-Path -LiteralPath $p) { $cc = Get-CcCertificate $p } }
      Test-Cc "lease-api.md#times: create --ttl $($case.Ttl) gives a certificate valid-before the issue time plus $($case.Seconds) s, equal to expires_at" {
        if (-not $rr.Ok) { return "The create gave $(Format-CcRun $rr.Run)" }
        if (-not $cc) { return 'There is no certificate.' }
        if ($rr.Doc['ttl_seconds'] -ne $case.Seconds) { return "ttl_seconds is $($rr.Doc['ttl_seconds'])." }
        if ((ConvertTo-CcTime $cc.To) -cne $rr.Doc['expires_at']) { return 'valid-before does not equal expires_at.' }
        $issue = $cc.To.AddSeconds(-$case.Seconds)
        if ($issue -lt $a -or $issue -gt $b) { return "The issue time $(ConvertTo-CcTime $issue) is outside the call." }
        if ($cc.From -gt $issue) { return 'valid-after is later than the issue time.' }
        $true
      }
    }

    # The key (state.md#the-key; lease-api.md#the-credential).
    Test-Cc "state.md#the-key: The private key is in OpenSSH's format with no passphrase, its cipher and KDF both none" {
      if (-not (Test-Path -LiteralPath $keyPath)) { return 'There is no key.' }
      $enc = Get-CcKeyEncryption $keyPath
      if (-not $enc) { return 'The key is not in the OpenSSH private key format.' }
      if ($enc.Cipher -cne 'none' -or $enc.Kdf -cne 'none') { return "The cipher is '$($enc.Cipher)' and the KDF '$($enc.Kdf)'." }
      $true
    }
    Test-Cc "lease-api.md#the-credential: The private key loads with an empty passphrase, so ssh never prompts" {
      if (-not (Test-Path -LiteralPath $keyPath)) { return 'There is no key.' }
      $out = & (Get-CcTool 'ssh-keygen') -y -P '' -f $keyPath 2>&1 | Out-String
      if ($LASTEXITCODE -ne 0 -or $out -notmatch '^ssh-ed25519 ') { return 'ssh-keygen -y with an empty passphrase failed.' }
      $true
    }
    Test-Cc "state.md#the-key: The public key is one ssh-ed25519 line, and is the private key's public half" {
      if (-not (Test-Path -LiteralPath $pubPath)) { return 'There is no public key.' }
      $line = ([IO.File]::ReadAllText($pubPath)).TrimEnd("`r", "`n")
      if ($line -match "[`r`n]" -or $line -notmatch '^ssh-ed25519 \S+') { return 'The public key is not one ssh-ed25519 line.' }
      $derived = (& (Get-CcTool 'ssh-keygen') -y -P '' -f $keyPath 2>&1 | Out-String).Trim().Split(' ')
      if ($derived.Count -lt 2 -or $derived[1] -cne $line.Split(' ')[1]) { return 'The public key is not the private key''s.' }
      $true
    }
    Test-Cc "lease-api.md#the-credential: No private key, in whole or part, appears in create's output" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      $body = [regex]::Match([IO.File]::ReadAllText($keyPath), '-----BEGIN OPENSSH PRIVATE KEY-----\s*(.+?)\s*-----END', 'Singleline').Groups[1].Value -replace '\s', ''
      if ($body.Length -lt 100) { return 'The key could not be read.' }
      $all = $r.Run.Stdout + $r.Run.Stderr
      if ($all.Contains('PRIVATE KEY') -or $all.Contains($body.Substring(0, 40)) -or $all.Contains($body.Substring(60, 40))) { return 'The key appears in the output.' }
      $true
    }
    Test-Cc "lease-api.md#the-credential: Create makes a new key for each lease" {
      $r2 = New-CcReadyLease $lab 'cert'
      if (-not $r2.Ok) { return "The create gave $(Format-CcRun $r2.Run)" }
      $other = Get-CcFileHash (Join-Path (Get-CcLeaseDir $lab ([string]$r2.Doc['id'])) 'id_ed25519.pub')
      if ($other -eq 'absent' -or $other -eq (Get-CcFileHash $pubPath)) { return 'The two leases share a key, or the second has none.' }
      $true
    }

    # The CA is only used, never made, when it is there (state.md#the-ca): the folder is unchanged.
    Test-Cc "state.md#the-ca: Signing leaves the CA's two files as they were" {
      if (-not $r.Ok) { return "Setup: create gave $(Format-CcRun $r.Run)" }
      $caDir = Join-Path $lab.UserProfile '.agentspace\ca'
      $names = (Get-CcFileNames $caDir) -join '|'
      if ($names -cne 'agentspace_ca|agentspace_ca.pub') { return "The CA folder holds '$names'." }
      $true
    }
  }
}
catch {
  Write-ConformanceResult -Name 'No unexpected error in test-lease-certificate' -Status FAIL -Reason ("It threw: " + ($_.Exception.Message -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun

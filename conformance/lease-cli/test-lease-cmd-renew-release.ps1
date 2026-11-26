#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.04-T03-S03, #0086): `agentspace lease renew` and `release`, as a
# caller sees them. It enforces docs/lease-api.md, "agentspace lease renew" and "agentspace lease
# release", "States", "The credential", "Concurrency and atomicity" and "Exit codes and errors",
# and docs/state.md, "Renewal" and "Release, and what it leaves".
#
# - It drives the program that AGENTSPACE_CMD names, only through the harness, as a separate
#   process, in a throwaway home with a throwaway CA. It reads no real folder, loads no prototype
#   code, and names no prototype path.
# - Each ready lease is made by `create`. The other records are reached by editing the state file
#   between commands, as docs/state.md documents it: an expired lease has its `expires_at` moved
#   into the past, and a pending one is written whole.
# - A failed write of the record is made by holding leases.json open for reading with no delete share,
#   which makes the rename fail (state.md, "Replacing a file"). The requirements the prototype does not
#   meet (the previous certificate kept, #0041 point 3; a release that fails part way) are not here.
#   See docs/conformance.md, "The files that are expected to fail".
# - Nothing connects anywhere, and nothing here needs a box or AWS.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-cmd-renew-release.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$none = '00000000-0000-4000-8000-000000000000'
$unknownId = '3b6c1d2e-5f40-4a8b-9c7d-1e2f3a4b5c6d'
$renew = 'lease-api.md#agentspace-lease-renew'
$release = 'lease-api.md#agentspace-lease-release'

# Waits until the clock is past the given whole second, so that two certificates cannot share an
# issue time (each call sets a new expiry from its own time, lease-api.md "renew").
# The wait is bounded to 5 seconds, because a time the program printed is untrusted: one far in the
# future would otherwise hold the check for ever.
function Wait-CcPast([DateTime]$T) {
  $limit = [DateTime]::UtcNow.AddSeconds(5)
  while ((Get-CcNowSecond) -le $T -and [DateTime]::UtcNow -lt $limit) { Start-Sleep -Milliseconds 100 }
}

function Get-CcIssue($Doc) {
  if (-not $Doc -or -not $Doc['expires_at']) { return [DateTime]::UtcNow }
  (ConvertFrom-CcTime $Doc['expires_at']).AddSeconds(-[double]$Doc['ttl_seconds'])
}

function Get-CcLeaseFacts($Lab, $Doc) {
  $id = if ($Doc) { [string]$Doc['id'] } else { $none }
  $dir = Get-CcLeaseDir $Lab $id
  $alias = if ($Doc -and $Doc['ssh']) { [string]$Doc['ssh']['host_alias'] } else { '' }
  [pscustomobject]@{
    Id = $id; Dir = $dir; Alias = $alias
    AliasPath = (Get-CcAliasPath $Lab $alias)
    Key = (Get-CcFileHash (Join-Path $dir 'id_ed25519')); Pub = (Get-CcFileHash (Join-Path $dir 'id_ed25519.pub'))
    Cert = (Get-CcFileHash (Join-Path $dir 'id_ed25519-cert.pub')); AliasHash = (Get-CcFileHash (Get-CcAliasPath $Lab $alias))
    Config = (Get-CcFileHash (Join-Path $Lab.SshDir 'config')); Names = ((Get-CcFileNames $dir) -join '|')
    StateNames = ((Get-CcFileNames (Split-Path -Parent (Get-CcStatePath $Lab))) -join '|')
    State = (Get-CcStateCanonical $Lab)
  }
}

$lab = New-ConformanceHome
try {
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$renew and $release" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {
    $threeNames = 'id_ed25519|id_ed25519-cert.pub|id_ed25519.pub'

    # ---- renew: a ready lease -------------------------------------------------------------------
    $r1 = New-CcReadyLease $lab 'alpha'
    $d1 = $r1.Doc
    Test-Cc "$renew`: Setup, a ready lease is made by create" { if ($r1.Ok) { $true } else { "The create gave $(Format-CcRun $r1.Run)" } }
    $f1 = Get-CcLeaseFacts $lab $d1
    $cert1 = if ($d1) { Get-CcCertificate (Join-Path $f1.Dir 'id_ed25519-cert.pub') } else { $null }
    if ($d1) { Wait-CcPast (Get-CcIssue $d1) }
    $recBefore1 = Get-CcRecord $lab $f1.Id
    $t0 = Get-CcNowSecond
    $run = Invoke-CcLease $lab @('renew', $f1.Id)
    $t1 = Get-CcNowSecond
    $d = ConvertFrom-CcJson $run.Stdout
    $after = Get-CcLeaseFacts $lab $d1
    Test-Cc "$renew`: Renew of a ready lease exits 0 with one valid lease document" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($d['state'] -cne 'ready') { return "The state was '$($d['state'])'." }
      $true
    }
    Test-Cc "$renew`: The ID, project, alias, user and created_at do not change" {
      foreach ($k in 'id', 'project', 'platform', 'profile', 'size', 'created_at') {
        if ($d[$k] -cne $d1[$k]) { return "$k changed from '$($d1[$k])' to '$($d[$k])'." }
      }
      if ($d['ssh']['host_alias'] -cne $d1['ssh']['host_alias'] -or $d['ssh']['user'] -cne $d1['ssh']['user']) { return 'The alias or the user changed.' }
      $true
    }
    Test-Cc "$renew`: Without --ttl the lease's ttl_seconds is used again" {
      if ($d['ttl_seconds'] -ne 28800 -or $d1['ttl_seconds'] -ne 28800) { return "ttl_seconds was $($d['ttl_seconds']), and create's was $($d1['ttl_seconds'])." }
      $true
    }
    Test-Cc "$renew`: expires_at is the issue time plus the TTL, and the issue time is within the call (own clock)" {
      $issue = Get-CcIssue $d
      if ($issue -lt $t0 -or $issue -gt $t1) { return "The issue time $(ConvertTo-CcTime $issue) is outside $(ConvertTo-CcTime $t0) to $(ConvertTo-CcTime $t1)." }
      if ((ConvertFrom-CcTime $d['expires_at']) -le (ConvertFrom-CcTime $d1['expires_at'])) { return 'expires_at did not move later.' }
      $true
    }
    Test-Cc "$renew`: The certificate is replaced, and its valid-before time equals expires_at (state.md#renewal)" {
      if ($after.Cert -eq 'absent' -or $after.Cert -eq $f1.Cert) { return 'The certificate file was not replaced.' }
      $c = Get-CcCertificate (Join-Path $after.Dir 'id_ed25519-cert.pub')
      if ((ConvertTo-CcTime $c.To) -cne $d['expires_at']) { return "valid-before is $(ConvertTo-CcTime $c.To), and expires_at is $($d['expires_at'])." }
      if ($c.KeyId -cne $f1.Id) { return "The Key ID is '$($c.KeyId)'." }
      $true
    }
    Test-Cc "$renew`: The private key, the public key, the alias file and the config are byte-identical (lease-api.md#the-credential)" {
      if ($f1.Key -eq 'absent') { return 'There was no key to compare.' }
      if ($after.Key -ne $f1.Key) { return 'The private key changed.' }
      if ($after.Pub -ne $f1.Pub) { return 'The public key changed.' }
      if ($f1.AliasHash -eq 'absent' -or $after.AliasHash -ne $f1.AliasHash) { return 'The alias file changed or was absent.' }
      if ($after.Config -ne $f1.Config) { return 'The config changed.' }
      $true
    }
    Test-Cc "$renew`: The lease folder holds the three files and no temporary file is left (state.md#the-lease-folder)" {
      if ($after.Names -cne $threeNames) { return "The folder holds '$($after.Names)'." }
      $true
    }
    Test-Cc "$renew`: The state file records the new expires_at and ttl_seconds and changes nothing else, the driver included" {
      $rec = Get-CcRecord $lab $f1.Id
      if (-not $rec) { return 'There is no record.' }
      if ($rec['expires_at'] -cne $d['expires_at'] -or $rec['ttl_seconds'] -ne $d['ttl_seconds']) { return 'The record does not carry the new expiry.' }
      if (-not $rec['driver']) { return 'The record lost its driver.' }
      if (-not $recBefore1) { return 'Setup: there was no record to compare.' }
      # The whole record, every member, as it was before the renew but for the two it MUST change
      # (state.md, "The lease state file", renew): the driver and the times of creation included.
      $copy = ConvertFrom-CcJson (ConvertTo-CcCanonical $rec)
      $copy['expires_at'] = $recBefore1['expires_at']; $copy['ttl_seconds'] = $recBefore1['ttl_seconds']
      if ((ConvertTo-CcCanonical $copy) -cne (ConvertTo-CcCanonical $recBefore1)) { return 'The record differs from the one before the renew in more than expires_at and ttl_seconds.' }
      if (-not (Test-CcStateFile $lab)) { return 'The state file does not validate against its schema.' }
      $true
    }
    $rec0 = Get-CcRecord $lab $f1.Id
    $driverBefore = if ($rec0) { [string]$rec0['driver'] } else { '' }

    # A new TTL, shorter than the one before: it replaces the expiry whether later or earlier.
    $t0 = Get-CcNowSecond
    Wait-CcPast (Get-CcIssue $d)
    $t0 = Get-CcNowSecond
    $run = Invoke-CcLease $lab @('renew', $f1.Id, '--ttl', '90m')
    $t1 = Get-CcNowSecond
    $dTtl = ConvertFrom-CcJson $run.Stdout
    Test-Cc "$renew`: renew --ttl 90m sets ttl_seconds 5400 and an expires_at that is the issue time plus 90 minutes, earlier than before" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($dTtl['ttl_seconds'] -ne 5400) { return "ttl_seconds was $($dTtl['ttl_seconds'])." }
      $issue = Get-CcIssue $dTtl
      if ($issue -lt $t0 -or $issue -gt $t1) { return "The issue time $(ConvertTo-CcTime $issue) is outside the call." }
      if ((ConvertFrom-CcTime $dTtl['expires_at']) -ge (ConvertFrom-CcTime $d['expires_at'])) { return 'A shorter TTL did not give an earlier expires_at.' }
      $c = Get-CcCertificate (Join-Path $f1.Dir 'id_ed25519-cert.pub')
      if ((ConvertTo-CcTime $c.To) -cne $dTtl['expires_at']) { return 'valid-before does not equal expires_at.' }
      $true
    }
    Test-Cc "$renew`: The driver in the record is kept across renews (state.md, the lease state file)" {
      if (-not $driverBefore) { return 'The record had no driver.' }
      if ([string](Get-CcRecord $lab $f1.Id)['driver'] -cne $driverBefore) { return 'The driver changed.' }
      $true
    }
    Wait-CcPast (Get-CcIssue $dTtl)
    $certBefore2 = Get-CcFileHash (Join-Path $f1.Dir 'id_ed25519-cert.pub')
    $run = Invoke-CcLease $lab @('renew', "  $($f1.Id.ToUpperInvariant())  ")
    $dTwo = ConvertFrom-CcJson $run.Stdout
    Test-Cc "$renew`: A second renew, with the ID in upper case and padded, reuses 5400 s, prints the lowercase ID and has its own window" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($dTwo['id'] -cne $f1.Id) { return "The ID was '$($dTwo['id'])'." }
      if ($dTwo['ttl_seconds'] -ne 5400) { return "ttl_seconds was $($dTwo['ttl_seconds'])." }
      if ((ConvertFrom-CcTime $dTwo['expires_at']) -le (ConvertFrom-CcTime $dTtl['expires_at'])) { return 'A later renew did not give a later expires_at.' }
      if ((Get-CcFileHash (Join-Path $f1.Dir 'id_ed25519-cert.pub')) -eq $certBefore2) { return 'The certificate was not replaced.' }
      $c = Get-CcCertificate (Join-Path $f1.Dir 'id_ed25519-cert.pub')
      if ((ConvertTo-CcTime $c.To) -cne $dTwo['expires_at']) { return 'valid-before does not equal expires_at.' }
      $true
    }

    # ---- renew: values refused before anything changes ------------------------------------------
    $before = Get-CcLeaseFacts $lab $d1
    $run = Invoke-CcLease $lab @('renew', $f1.Id, '--ttl', '59s')
    $now = Get-CcLeaseFacts $lab $d1
    Test-Cc "$renew`: A TTL below 1 minute is out_of_range (exit 2, field ttl) and changes nothing" {
      if (-not (Test-CcFailure $run 2 'out_of_range' @{ field = 'ttl' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ($now.State -cne $before.State -or $now.Cert -ne $before.Cert) { return 'The state file or the certificate changed.' }
      $true
    }
    $run = Invoke-CcLease $lab @('renew', $f1.Id, '--ttl', '8')
    Test-Cc "$renew`: A bare number TTL is invalid_value (exit 2, field ttl) and changes nothing" {
      if (-not (Test-CcFailure $run 2 'invalid_value' @{ field = 'ttl' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ((Get-CcLeaseFacts $lab $d1).Cert -ne $before.Cert) { return 'The certificate changed.' }
      $true
    }
    $run = Invoke-CcLease $lab @('renew', $unknownId, '--ttl', '8')
    Test-Cc "lease-api.md#exit-codes-and-errors: renew <unknown id> --ttl 8 is invalid_value for ttl, ahead of not_found" {
      if (-not (Test-CcFailure $run 2 'invalid_value' @{ field = 'ttl' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }
    $run = Invoke-CcLease $lab @('renew', $unknownId.ToUpperInvariant())
    Test-Cc "$renew`: Renew of an unknown lease is not_found (exit 3) naming the normalized ID" {
      if (-not (Test-CcFailure $run 3 'not_found' @{ lease_id = $unknownId })) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }

    # ---- renew: refused by state, and the lease is left as it was ----------------------------------
    $r2 = New-CcReadyLease $lab 'alpha'
    $d2 = $r2.Doc
    $f2 = Get-CcLeaseFacts $lab $d2
    if ($d2) { Edit-CcRecord $lab $f2.Id { param($rec) $rec['expires_at'] = '2020-01-01T00:00:00Z' } }
    $f2b = Get-CcLeaseFacts $lab $d2
    $run = Invoke-CcLease $lab @('renew', $f2.Id)
    $f2c = Get-CcLeaseFacts $lab $d2
    Test-Cc "$renew`: Renew of an expired lease is invalid_state (exit 4) naming expired in its state member, and changes nothing" {
      if (-not $r2.Ok) { return "Setup: create gave $(Format-CcRun $r2.Run)" }
      if (-not (Test-CcFailure $run 4 'invalid_state' @{ lease_id = $f2.Id; state = 'expired' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ($f2c.State -cne $f2b.State -or $f2c.Cert -ne $f2b.Cert -or $f2b.Cert -eq 'absent') { return 'The state file or the certificate changed.' }
      $true
    }
    $run = Invoke-CcLease $lab @('renew', $f2.Id, '--ttl', '48h')
    Test-Cc "lease-api.md#exit-codes-and-errors: renew <expired id> --ttl 48h is out_of_range (exit 2), ahead of invalid_state" {
      if (-not (Test-CcFailure $run 2 'out_of_range' @{ field = 'ttl' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }

    $r3 = New-CcReadyLease $lab 'beta'
    $d3 = $r3.Doc
    $f3 = Get-CcLeaseFacts $lab $d3
    $run3 = Invoke-CcLease $lab @('release', $f3.Id)
    $f3b = Get-CcLeaseFacts $lab $d3
    $run = Invoke-CcLease $lab @('renew', $f3.Id)
    $f3c = Get-CcLeaseFacts $lab $d3
    Test-Cc "$renew`: Renew of a released lease is invalid_state (exit 4) naming released, and changes nothing" {
      if ($run3.ExitCode -ne 0) { return "Setup: release gave $(Format-CcRun $run3)" }
      if (-not (Test-CcFailure $run 4 'invalid_state' @{ lease_id = $f3.Id; state = 'released' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ($f3c.State -cne $f3b.State) { return 'The state file changed.' }
      $true
    }

    $unsat = Invoke-CcLease $lab @('create', '--project', 'gamma', '--size', 'xxl')
    $du = ConvertFrom-CcJson $unsat.Stdout
    $uId = if ($du) { [string]$du['id'] } else { $none }
    $run = Invoke-CcLease $lab @('renew', $uId)
    Test-Cc "$renew`: Renew of an unsatisfiable lease is invalid_state (exit 4) naming unsatisfiable" {
      if (-not $du -or $du['state'] -cne 'unsatisfiable') { return "Setup: create gave $(Format-CcRun $unsat)" }
      if (-not (Test-CcFailure $run 4 'invalid_state' @{ lease_id = $uId; state = 'unsatisfiable' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }

    $pendId = [guid]::NewGuid().ToString()
    $pendRecord = [ordered]@{
      id = $pendId; project = 'gamma'; state = 'pending'; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'
      ttl_seconds = 3600; created_at = (ConvertTo-CcTime ([DateTime]::UtcNow))
    }
    if ($driverBefore) { $pendRecord['driver'] = $driverBefore }
    Add-CcRecord $lab $pendRecord
    $statePending = Get-CcStateCanonical $lab
    $run = Invoke-CcLease $lab @('renew', $pendId)
    Test-Cc "$renew`: Renew of a pending lease is invalid_state (exit 4) naming pending, and changes nothing" {
      if (-not (Test-CcFailure $run 4 'invalid_state' @{ lease_id = $pendId; state = 'pending' })) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ((Get-CcStateCanonical $lab) -cne $statePending) { return 'The state file changed.' }
      $true
    }

    # ---- renew: a failed renew leaves the lease as it was (#0041) ---------------------------------
    $r4 = New-CcReadyLease $lab 'delta'
    $d4 = $r4.Doc
    $f4 = Get-CcLeaseFacts $lab $d4
    # Both halves of the key pair go, so that no certificate can be issued, and the key MUST NOT change
    # (lease-api.md "renew", "The key, the alias, the user, the ID and `created_at` do not change"). The
    # order in which a build reads the two files is state.md's SHOULD, and is not checked.
    if ($d4) { foreach ($half in 'id_ed25519', 'id_ed25519.pub') { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path (Join-Path $f4.Dir $half)) -Force } }
    $f4b = Get-CcLeaseFacts $lab $d4
    $run = Invoke-CcLease $lab @('renew', $f4.Id)
    $f4c = Get-CcLeaseFacts $lab $d4
    Test-Cc "$renew`: Renew of a lease whose key pair is missing is internal (exit 1), changes nothing, makes no new key and leaves no temporary file in the lease folder or the state folder (the key MUST NOT change)" {
      if (-not $r4.Ok) { return "Setup: create gave $(Format-CcRun $r4.Run)" }
      if ($f4b.Key -ne 'absent' -or $f4b.Pub -ne 'absent' -or $f4b.Cert -eq 'absent') { return 'Setup: the key pair was not removed, or there was no certificate.' }
      if (-not (Test-CcFailure $run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ($f4c.State -cne $f4b.State) { return 'The state file changed.' }
      if ($f4c.Cert -ne $f4b.Cert) { return 'The certificate changed.' }
      if ($f4c.Names -cne $f4b.Names) { return "The folder changed from '$($f4b.Names)' to '$($f4c.Names)'." }
      if ($f4c.StateNames -cne $f4b.StateNames) { return "The state folder changed from '$($f4b.StateNames)' to '$($f4c.StateNames)'." }
      $true
    }

    # A ready lease whose whole folder is gone cannot be renewed: with no key pair no certificate can be
    # issued, the key MUST NOT change, and lease-api.md says `internal` for a certificate that could not be
    # issued. A missing certificate alone is no error, and is not made here.
    $r5 = New-CcReadyLease $lab 'echo'
    $f5 = Get-CcLeaseFacts $lab $r5.Doc
    if ($r5.Ok) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $f5.Dir) -Recurse -Force }
    $f5b = Get-CcLeaseFacts $lab $r5.Doc
    $run = Invoke-CcLease $lab @('renew', $f5.Id)
    $f5c = Get-CcLeaseFacts $lab $r5.Doc
    Test-Cc "$renew`: Renew of a ready lease whose folder is gone is internal (exit 1), makes no folder, and changes nothing (the key MUST NOT change)" {
      if (-not $r5.Ok) { return "Setup: create gave $(Format-CcRun $r5.Run)" }
      if (Test-Path -LiteralPath $f5b.Dir) { return 'Setup: the folder was not removed.' }
      if (-not (Test-CcFailure $run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run)" }
      if (Test-Path -LiteralPath $f5c.Dir) { return 'The renew made the lease folder.' }
      if ($f5c.State -cne $f5b.State -or $f5c.AliasHash -ne $f5b.AliasHash -or $f5c.Config -ne $f5b.Config) { return 'The state file, the alias file or the config changed.' }
      $true
    }

    # A failed write of the record (state.md "Replacing a file": a rename fails when another process holds
    # the file without sharing delete). The check holds leases.json open for reading with no delete share,
    # which is what a reader that does not follow the rule would do. What the prototype shows, and a
    # conforming build MUST show, is checked here: `internal`, the record as it was, no temporary file and
    # the key pair, the alias and the config untouched. That the previous certificate is kept, byte for
    # byte, is #0041 point 3 and is a requirement the prototype does not meet (docs/conformance.md, "The
    # files that are expected to fail").
    $r6 = New-CcReadyLease $lab 'foxtrot'
    $f6 = Get-CcLeaseFacts $lab $r6.Doc
    $run6 = $null
    if ($r6.Ok) {
      $held6 = [IO.FileStream]::new((Assert-ConformancePath -ConformanceHome $lab -Path (Get-CcStatePath $lab)), [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
      try { $run6 = Invoke-CcLease $lab @('renew', $f6.Id) } finally { $held6.Dispose() }
    }
    $f6c = Get-CcLeaseFacts $lab $r6.Doc
    Test-Cc "$renew`: A renew whose record cannot be written is internal (exit 1), leaves the record as it was and no temporary file in the lease folder or the state folder, and keeps the key, the alias and the config (state.md#replacing-a-file)" {
      if (-not $r6.Ok) { return "Setup: create gave $(Format-CcRun $r6.Run)" }
      if (-not (Test-CcFailure $run6 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run6)" }
      if ($f6c.State -cne $f6.State) { return 'The state file changed.' }
      if ($f6c.Names -cne $threeNames) { return "The lease folder holds '$($f6c.Names)'." }
      if ($f6c.StateNames -cne $f6.StateNames) { return "The state folder changed from '$($f6.StateNames)' to '$($f6c.StateNames)'." }
      if ($f6c.Key -ne $f6.Key -or $f6c.Pub -ne $f6.Pub -or $f6c.AliasHash -ne $f6.AliasHash -or $f6c.Config -ne $f6.Config) { return 'The key pair, the alias file or the config changed.' }
      $true
    }
    $later = Invoke-CcLease $lab @('renew', $f6.Id)
    Test-Cc "$renew`: Once the file is free, the same renew succeeds (a failed one leaves nothing that blocks a later one)" {
      if (-not $r6.Ok) { return "Setup: create gave $(Format-CcRun $r6.Run)" }
      if (-not (Test-CcSuccess $later)) { return "The run was not a valid success: $(Format-CcRun $later)" }
      $true
    }

    # ---- release ------------------------------------------------------------------------------------
    $l1 = New-CcReadyLease $lab 'beta'
    $l1b = New-CcReadyLease $lab 'beta'
    $dl = $l1.Doc
    $fl = Get-CcLeaseFacts $lab $dl
    $fb = Get-CcLeaseFacts $lab $l1b.Doc
    Test-Cc "$release`: Setup, two ready leases with a folder and an alias file each" {
      if (-not ($l1.Ok -and $l1b.Ok)) { return 'The create did not make both leases.' }
      if (-not (Test-Path -LiteralPath $fl.Dir -PathType Container) -or $fl.AliasHash -eq 'absent') { return 'The lease has no folder or no alias file.' }
      if (-not (Test-Path -LiteralPath $fb.Dir -PathType Container) -or $fb.AliasHash -eq 'absent') { return 'The other lease has no folder or no alias file.' }
      $true
    }
    $run = Invoke-CcLease $lab @('release', $fl.Id.ToUpperInvariant())
    $dRel = ConvertFrom-CcJson $run.Stdout
    $fl2 = Get-CcLeaseFacts $lab $dl
    $fb2 = Get-CcLeaseFacts $lab $l1b.Doc
    Test-Cc "$release`: Release of a ready lease (ID in upper case) exits 0 with state released, released_at, no ssh, and expires_at kept" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($dRel['state'] -cne 'released' -or $dRel['id'] -cne $fl.Id) { return "State '$($dRel['state'])', id '$($dRel['id'])'." }
      if ($dRel.ContainsKey('ssh') -or -not $dRel.ContainsKey('released_at')) { return 'ssh is present, or released_at is missing.' }
      if ($dRel['expires_at'] -cne $dl['expires_at']) { return 'expires_at was not kept.' }
      foreach ($k in 'project', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at') { if ($dRel[$k] -cne $dl[$k]) { return "$k changed." } }
      $true
    }
    Test-Cc "$release`: Release deletes the lease folder (key and certificate) and the alias file (state.md#release-and-what-it-leaves)" {
      if ($fl.AliasHash -eq 'absent' -or $fl.Key -eq 'absent') { return 'Setup: there was nothing to delete.' }
      if (Test-Path -LiteralPath $fl2.Dir) { return 'The lease folder is still there.' }
      if (Test-Path -LiteralPath $fl.AliasPath) { return 'The alias file is still there.' }
      $true
    }
    Test-Cc "$release`: Release leaves config byte-identical, with its Include line, and leaves another lease's files alone" {
      if ($fl.Config -eq 'absent' -or $fl2.Config -ne $fl.Config) { return 'The config changed.' }
      if ($fb2.Key -ne $fb.Key -or $fb2.Cert -ne $fb.Cert -or $fb2.AliasHash -ne $fb.AliasHash -or $fb.AliasHash -eq 'absent') { return "The other lease's files changed." }
      $true
    }
    Test-Cc "$release`: The state file records released, without ssh, with the first released_at, and validates" {
      $rec = Get-CcRecord $lab $fl.Id
      if (-not $rec -or $rec['state'] -cne 'released' -or $rec.ContainsKey('ssh') -or $rec['released_at'] -cne $dRel['released_at']) { return 'The record is not the released one.' }
      if (-not (Test-CcStateFile $lab)) { return 'The state file does not validate.' }
      $true
    }
    Wait-CcPast (Get-CcNowSecond)
    $run = Invoke-CcLease $lab @('release', $fl.Id)
    $dRel2 = ConvertFrom-CcJson $run.Stdout
    Test-Cc "$release`: A second release succeeds and prints the same lease, with its first released_at (idempotent)" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((ConvertTo-CcCanonical $dRel2) -cne (ConvertTo-CcCanonical $dRel)) { return 'The second document differs from the first.' }
      $true
    }
    $run = Invoke-CcLease $lab @('release', $unknownId)
    Test-Cc "$release`: Release of an unknown lease is not_found (exit 3)" {
      if (-not (Test-CcFailure $run 3 'not_found' @{ lease_id = $unknownId })) { return "Not the expected failure: $(Format-CcRun $run)" }
      $true
    }

    # An expired lease is released, with its files, and keeps its expires_at.
    $f2d = Get-CcLeaseFacts $lab $d2
    $run = Invoke-CcLease $lab @('release', $f2.Id)
    $dExp = ConvertFrom-CcJson $run.Stdout
    Test-Cc "$release`: Release of an expired lease gives released, keeps its expires_at, and deletes its folder and alias file" {
      if ($f2d.Key -eq 'absent' -or $f2d.AliasHash -eq 'absent') { return 'Setup: the expired lease had no files.' }
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($dExp['state'] -cne 'released' -or $dExp['expires_at'] -cne '2020-01-01T00:00:00Z') { return 'The state or the expires_at is wrong.' }
      if ((Test-Path -LiteralPath $f2.Dir) -or (Test-Path -LiteralPath $f2.AliasPath)) { return 'The folder or the alias file is still there.' }
      $true
    }

    # A pending lease is released without any file, and has no expires_at.
    $run = Invoke-CcLease $lab @('release', $pendId)
    $dPend = ConvertFrom-CcJson $run.Stdout
    Test-Cc "$release`: Release of a pending lease gives released with released_at and no expires_at" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ($dPend['state'] -cne 'released' -or -not $dPend.ContainsKey('released_at') -or $dPend.ContainsKey('expires_at') -or $dPend.ContainsKey('ssh')) { return 'The document is not a released pending lease.' }
      $true
    }

    # An unsatisfiable lease is returned unchanged.
    $run = Invoke-CcLease $lab @('release', $uId)
    Test-Cc "$release`: Release of an unsatisfiable lease changes nothing and prints it unchanged" {
      if (-not $du -or $du['state'] -cne 'unsatisfiable') { return 'Setup: there was no unsatisfiable lease.' }
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((ConvertTo-CcCanonical (ConvertFrom-CcJson $run.Stdout)) -cne (ConvertTo-CcCanonical $du)) { return 'The document differs from the one create printed.' }
      $true
    }

    # A release finishes when a file is already gone: a missing alias file and a missing folder are not errors.
    $l2 = New-CcReadyLease $lab 'beta'
    $fm = Get-CcLeaseFacts $lab $l2.Doc
    if ($l2.Ok) {
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $fm.AliasPath) -Force
      Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $fm.Dir) -Recurse -Force
    }
    $run = Invoke-CcLease $lab @('release', $fm.Id)
    Test-Cc "$release`: Release succeeds when the alias file and the lease folder are already gone" {
      if (-not $l2.Ok) { return "Setup: create gave $(Format-CcRun $l2.Run)" }
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((ConvertFrom-CcJson $run.Stdout)['state'] -cne 'released') { return 'The state is not released.' }
      $true
    }

    # A foreign file at the alias's place is never deleted, and the lease stays as it was.
    $l3 = New-CcReadyLease $lab 'beta'
    $fo = Get-CcLeaseFacts $lab $l3.Doc
    $foreign = "# not an agentspace alias`n"
    $aliasOriginal = if ($l3.Ok) { [IO.File]::ReadAllBytes($fo.AliasPath) } else { [byte[]]@() }
    if ($l3.Ok) { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $fo.AliasPath), [Text.UTF8Encoding]::new($false).GetBytes($foreign)) }
    $foBefore = Get-CcLeaseFacts $lab $l3.Doc
    $run = Invoke-CcLease $lab @('release', $fo.Id)
    $foAfter = Get-CcLeaseFacts $lab $l3.Doc
    Test-Cc "$release`: Release is internal (exit 1) when the alias file is another's, deletes nothing, and leaves the lease ready (driver.md, Release a lease)" {
      if (-not $l3.Ok) { return "Setup: create gave $(Format-CcRun $l3.Run)" }
      if (-not (Test-CcFailure $run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run)" }
      if ([IO.File]::ReadAllText($fo.AliasPath) -cne $foreign) { return 'The foreign alias file was changed or deleted.' }
      if (-not (Test-Path -LiteralPath $fo.Dir) -or $foAfter.Key -ne $foBefore.Key -or $foAfter.Cert -ne $foBefore.Cert) { return 'The lease folder or its files changed.' }
      if ($foAfter.State -cne $foBefore.State) { return 'The state file changed.' }
      $true
    }
    if ($l3.Ok) { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $fo.AliasPath), $aliasOriginal) }
    $run = Invoke-CcLease $lab @('release', $fo.Id)
    Test-Cc "$release`: Once the alias file is the lease's again, release finishes and deletes the folder and the alias file" {
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((Test-Path -LiteralPath $fo.Dir) -or (Test-Path -LiteralPath $fo.AliasPath)) { return 'A file is still there.' }
      $true
    }

    # The lease whose key is gone (above) is still releasable (#0041).
    $run = Invoke-CcLease $lab @('release', $f4.Id)
    Test-Cc "$release`: A ready lease whose key is missing is still releasable (lease-api.md#agentspace-lease-renew, #0041)" {
      if (-not $r4.Ok) { return "Setup: create gave $(Format-CcRun $r4.Run)" }
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if (Test-Path -LiteralPath $f4.Dir) { return 'The lease folder is still there.' }
      $true
    }

    # Release needs no CA: it works on a machine whose CA is missing, and does not make one.
    $l4 = New-CcReadyLease $lab 'beta'
    $fc = Get-CcLeaseFacts $lab $l4.Doc
    $caDir = Join-Path $lab.UserProfile '.agentspace\ca'
    if ($l4.Ok -and (Test-Path -LiteralPath $caDir)) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $lab -Path $caDir) -Recurse -Force }
    $run = Invoke-CcLease $lab @('release', $fc.Id)
    Test-Cc "$release`: Release needs no CA, and works with the CA missing, without creating one" {
      if (-not $l4.Ok) { return "Setup: create gave $(Format-CcRun $l4.Run)" }
      if (-not (Test-CcSuccess $run)) { return "The run was not a valid success: $(Format-CcRun $run)" }
      if ((ConvertFrom-CcJson $run.Stdout)['state'] -cne 'released' -or (Test-Path -LiteralPath $fc.Dir)) { return 'The lease was not released and deleted.' }
      if (Test-Path -LiteralPath $caDir) { return 'A CA was created.' }
      $true
    }

    Test-Cc "lease-api.md#output: The state file validates against its schema after the run" {
      if (-not $r1.Ok) { return "Setup: create gave $(Format-CcRun $r1.Run)" }
      if (Test-CcStateFile $lab) { $true } else { 'The state file does not validate.' }
    }
  }
}
catch {
  Write-ConformanceResult -Name 'No unexpected error in test-lease-cmd-renew-release' -Status FAIL -Reason ("It threw: " + ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check under D34 (E0.04-T03-S03, #0086): the alias file and the `Include` line that a
# ready `create` writes into the caller's OpenSSH client folder. It enforces docs/lease-api.md, "The
# credential" and "Environment", and docs/state.md, "The alias file", "The `Include` line" and
# "Replacing a file".
#
# - The client folder is the check's throwaway AGENTSPACE_SSH_DIR. Each case prepares it with one
#   config shape (LF, CRLF, a byte order mark, none, empty, no final line ending, the line already
#   there, the line misplaced), runs a ready `create`, and compares bytes.
# - `ssh -G -F <alias file> <alias>` is the oracle for what an alias file means. ssh is a program
#   outside agentspace, and it is told to read only the alias file, so nothing of the real
#   ~/.ssh is read.
# - Every ready `create` gets AGENTSPACE_PROXY_COMMAND, through the harness's -Environment (the harness
#   removes every AGENTSPACE_* variable the caller holds), except the one default-transport case, which
#   removes it with a $null value. That case seeds no instance-ID cache, and a conforming build needs
#   none (D21; docs/driver.md, "How it is reached").
# - Nothing connects anywhere.
#
# Run: pwsh -NoProfile -File conformance/lease-cli/test-lease-alias-file.ps1
# Exit: 0 when every check passed, 1 when any failed, 2 when none failed and any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-check-support.ps1')

$aliasDoc = 'state.md#the-alias-file'
$inc = 'state.md#the-include-line'
$utf8 = [Text.UTF8Encoding]::new($false)

function Test-CcBytesEqual([byte[]]$A, [byte[]]$B) { [Linq.Enumerable]::SequenceEqual($A, $B) }
function Get-CcBytes([string]$Path) { if (Test-Path -LiteralPath $Path -PathType Leaf) { , [IO.File]::ReadAllBytes($Path) } else { $null } }

# Prepares the ssh folder with a config (or none), runs a ready create, and returns what is left.
function Invoke-CcConfigCase {
  param($Lab, [byte[]]$Config, [string]$Project = 'cfg', [hashtable]$EnvVars = @{})
  Reset-CcSshDir $Lab
  $cfg = Join-Path $Lab.SshDir 'config'
  if ($null -ne $Config) { [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $Lab -Path $cfg), $Config) }
  $r = New-CcReadyLease -Lab $Lab -Project $Project -EnvVars $EnvVars
  $names = Get-CcFileNames $Lab.SshDir
  [pscustomobject]@{
    Lease = $r; Config = (Get-CcBytes $cfg); Names = $names
    Backups = @($names | Where-Object { $_ -like 'config.agentspace-backup-*' })
    AliasDir = (Get-CcFileNames (Join-Path $Lab.SshDir 'agentspace.d'))
  }
}

function Get-CcBackupBytes($Lab, $Case) { if ($Case.Backups.Count -eq 1) { Get-CcBytes (Join-Path $Lab.SshDir $Case.Backups[0]) } else { $null } }

$lab = New-ConformanceHome
try {
  if (-not $lab.CaReady) {
    Write-ConformanceResult -Name "$aliasDoc and $inc" -Status SKIPPED -Reason "No throwaway CA could be made: $($lab.CaReason)"
  }
  else {
    $line = 'Include agentspace.d/*'

    # ---- The Include line, on each shape of config -------------------------------------------------
    $origLf = $utf8.GetBytes("# My hosts`nHost agentspace`n  User ubuntu`n")
    $c = Invoke-CcConfigCase $lab $origLf
    Test-Cc "$inc`: On an LF config the line is added first, then every original byte, and the config is backed up byte for byte" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not (Test-CcBytesEqual $c.Config ($utf8.GetBytes("$line`n") + $origLf))) { return 'The config is not the line, an LF, and the original bytes.' }
      if ($c.Backups.Count -ne 1 -or -not (Test-CcBytesEqual (Get-CcBackupBytes $lab $c) $origLf)) { return 'There is not exactly one backup equal to the original.' }
      if ($c.Backups[0] -notmatch '^config\.agentspace-backup-\d{8}T\d{6}Z$') { return "The backup's name is '$($c.Backups[0])'." }
      $true
    }
    Test-Cc "$inc`: agentspace.d holds the alias file only, and no temporary file is left in the ssh folder (state.md#replacing-a-file)" {
      if (-not $c.Lease.Ok) { return 'There was no lease.' }
      if (($c.AliasDir -join '|') -cne $c.Lease.Doc['ssh']['host_alias']) { return "agentspace.d holds '$($c.AliasDir -join '|')'." }
      if (@($c.Names | Where-Object { $_ -like '*tmp*' }).Count -ne 0) { return "The ssh folder holds '$($c.Names -join '|')'." }
      $true
    }
    # A second create finds the line and changes nothing.
    $before = Get-CcBytes (Join-Path $lab.SshDir 'config')
    $r2 = New-CcReadyLease $lab 'cfg'
    $after = Get-CcBytes (Join-Path $lab.SshDir 'config')
    Test-Cc "$inc`: A second create finds the line, and leaves the config alone and makes no second backup" {
      if (-not $r2.Ok) { return "The create gave $(Format-CcRun $r2.Run)" }
      if (-not $before -or -not (Test-CcBytesEqual $before $after)) { return 'The config changed.' }
      if (@(Get-CcFileNames $lab.SshDir | Where-Object { $_ -like 'config.agentspace-backup-*' }).Count -ne 1) { return 'There is not exactly one backup.' }
      $true
    }

    $origCrlf = $utf8.GetBytes("# CRLF`r`nHost a`r`n  User u`r`n")
    $c = Invoke-CcConfigCase $lab $origCrlf
    Test-Cc "$inc`: On a CRLF config the line gets a CR LF ending, and the rest is byte-identical" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not (Test-CcBytesEqual $c.Config ($utf8.GetBytes("$line`r`n") + $origCrlf))) { return 'The config is not the line, a CR LF, and the original bytes.' }
      if ($c.Backups.Count -ne 1 -or -not (Test-CcBytesEqual (Get-CcBackupBytes $lab $c) $origCrlf)) { return 'There is not exactly one backup equal to the original.' }
      $true
    }

    $bom = [byte[]](0xEF, 0xBB, 0xBF)
    $origBom = $bom + $utf8.GetBytes("Host a`n")
    $c = Invoke-CcConfigCase $lab $origBom
    Test-Cc "$inc`: A byte order mark stays first, and the line follows it" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not (Test-CcBytesEqual $c.Config ($bom + $utf8.GetBytes("$line`n") + $utf8.GetBytes("Host a`n")))) { return 'The config is not the mark, the line, and the original after the mark.' }
      $true
    }

    $c = Invoke-CcConfigCase $lab $null
    Test-Cc "$inc`: With no config at all it is created holding exactly the line and one LF, and no backup is made" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not $c.Config -or -not (Test-CcBytesEqual $c.Config $utf8.GetBytes("$line`n"))) { return 'The config is not exactly the line and an LF.' }
      if ($c.Backups.Count -ne 0) { return 'A backup was made.' }
      $true
    }

    $c = Invoke-CcConfigCase $lab ([byte[]]@())
    Test-Cc "$inc`: An empty config (0 bytes) becomes the line and one LF" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not $c.Config -or -not (Test-CcBytesEqual $c.Config $utf8.GetBytes("$line`n"))) { return 'The config is not exactly the line and an LF.' }
      $true
    }

    $origNoEnd = $utf8.GetBytes("Host a`n  User u")
    $c = Invoke-CcConfigCase $lab $origNoEnd
    Test-Cc "$inc`: A config with no final line ending keeps none" {
      if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
      if (-not (Test-CcBytesEqual $c.Config ($utf8.GetBytes("$line`n") + $origNoEnd))) { return 'The config is not the line, an LF, and the original bytes unchanged.' }
      $true
    }

    foreach ($already in @(
        @{ Name = 'on its first line'; Text = "Include agentspace.d/*`nHost a`n" },
        @{ Name = 'above the first Host line, after a comment, in other case and with tabs'; Text = "# mine`nINCLUDE`tagentspace.d/*`nHost a`n" },
        @{ Name = 'above a Match line, with CR LF'; Text = "Include   agentspace.d/*`r`nMatch all`r`n  User u`r`n" })) {
      $orig = $utf8.GetBytes($already.Text)
      $c = Invoke-CcConfigCase $lab $orig
      Test-Cc "$inc`: A line already there, $($already.Name), is found: the config is byte-identical and there is no backup" {
        if (-not $c.Lease.Ok) { return "The create gave $(Format-CcRun $c.Lease.Run)" }
        if (-not (Test-CcBytesEqual $c.Config $orig)) { return 'The config changed.' }
        if ($c.Backups.Count -ne 0) { return 'A backup was made.' }
        if (($c.AliasDir -join '|') -cne $c.Lease.Doc['ssh']['host_alias']) { return 'The alias file is not in agentspace.d.' }
        $true
      }
    }

    # A line below a Host block belongs to that block: create fails, and nothing is left.
    $misplaced = $utf8.GetBytes("Host a`n  User u`nInclude agentspace.d/*`n")
    Reset-CcSshDir $lab
    $cfgPath = Join-Path $lab.SshDir 'config'
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $cfgPath), $misplaced)
    $leasesDir = Join-Path $lab.UserProfile '.agentspace\leases'
    $snapBefore = "$((Get-CcFileNames $leasesDir) -join '|');$(Get-CcStateCanonical $lab)"
    $run = Invoke-CcLease $lab @('create', '--project', 'cfg')
    $snapAfter = "$((Get-CcFileNames $leasesDir) -join '|');$(Get-CcStateCanonical $lab)"
    Test-Cc "$inc`: An Include line below a Host line makes create fail with internal, and leaves no lease, alias, backup or record" {
      if (-not (Test-CcFailure $run 1 'internal')) { return "Not the expected failure: $(Format-CcRun $run)" }
      if (-not (Test-CcBytesEqual (Get-CcBytes $cfgPath) $misplaced)) { return 'The config changed.' }
      if (@(Get-CcFileNames $lab.SshDir | Where-Object { $_ -like 'config.agentspace-*' }).Count -ne 0) { return 'A backup or temporary file was left.' }
      if ((Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')).Count -ne 0) { return 'An alias file was left.' }
      if ($snapAfter -cne $snapBefore) { return 'The lease folders or the state file changed.' }
      $true
    }
    [IO.File]::WriteAllBytes((Assert-ConformancePath -ConformanceHome $lab -Path $cfgPath), $utf8.GetBytes("Include agentspace.d/*`nHost a`n  User u`n"))
    $r3 = New-CcReadyLease $lab 'cfg'
    Test-Cc "lease-api.md#concurrency-and-atomicity: A later create is not blocked by the failed one" {
      if ($r3.Ok) { $true } else { "The create gave $(Format-CcRun $r3.Run)" }
    }

    # A ProxyCommand that holds a CR or a LF MUST be refused, writing nothing, because a line break would add
    # directives to the block (state.md, "The alias file"). It reaches the program through the variable
    # lease-api.md, "Environment", documents. create fails like any step that fails: with internal, and
    # leaving no lease folder, no alias file and no record (state.md, "Creation, in order").
    foreach ($bad in @(
        @{ Name = 'a line feed'; Value = "agentspace-conformance-proxy`nHost evil" },
        @{ Name = 'a carriage return'; Value = "agentspace-conformance-proxy`rHost evil" },
        @{ Name = 'a CR LF'; Value = "agentspace-conformance-proxy`r`nProxyCommand evil" })) {
      Reset-CcSshDir $lab
      $leasesDirB = Join-Path $lab.UserProfile '.agentspace\leases'
      $snapB = "$((Get-CcFileNames $leasesDirB) -join '|');$((Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')) -join '|');$(Get-CcStateCanonical $lab)"
      $runB = Invoke-CcLease $lab @('create', '--project', 'crlf') @{ AGENTSPACE_PROXY_COMMAND = $bad.Value }
      $snapAfterB = "$((Get-CcFileNames $leasesDirB) -join '|');$((Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')) -join '|');$(Get-CcStateCanonical $lab)"
      Test-Cc "$aliasDoc`: An AGENTSPACE_PROXY_COMMAND that holds $($bad.Name) is refused: create fails with internal and leaves no lease folder, no alias file and no record" {
        if (-not (Test-CcFailure $runB 1 'internal')) { return "Not the expected failure: $(Format-CcRun $runB)" }
        if ($snapAfterB -cne $snapB) { return 'The lease folders, the alias folder or the state file changed.' }
        $true
      }
    }

    # ---- The alias file ------------------------------------------------------------------------------
    Reset-CcSshDir $lab
    $r = New-CcReadyLease $lab 'SwimCoach'
    $d = $r.Doc
    $id = if ($d) { [string]$d['id'] } else { '00000000-0000-4000-8000-000000000000' }
    $alias = if ($d) { [string]$d['ssh']['host_alias'] } else { '' }
    $aliasPath = Get-CcAliasPath $lab $alias
    $bytes = Get-CcBytes $aliasPath
    $text = if ($bytes) { $utf8.GetString($bytes) } else { '' }
    $user = if ($d) { [string]$d['ssh']['user'] } else { '' }

    Test-Cc "lease-api.md#fields: The alias is agentspace-<project>-<first 8 hex of id>, in lower case, and the file has that name" {
      if (-not $r.Ok) { return "The create gave $(Format-CcRun $r.Run)" }
      if ($alias -cne "agentspace-swimcoach-$($id.Substring(0, 8))") { return "The alias is '$alias'." }
      if (-not $bytes) { return 'There is no alias file by that name in agentspace.d.' }
      $true
    }
    Test-Cc "$aliasDoc`: The file is UTF-8 with no byte order mark, LF line endings only, and ends in an LF" {
      if (-not $bytes) { return 'There is no alias file.' }
      if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { return 'It starts with a byte order mark.' }
      if ($bytes -contains 13) { return 'It holds a CR.' }
      if ($bytes[$bytes.Length - 1] -ne 10) { return 'It does not end in an LF.' }
      $true
    }
    Test-Cc "$aliasDoc`: The first line is the owner mark naming the lease ID, then a semicolon and a comment" {
      if (-not $text) { return 'There is no alias file.' }
      $first = $text.Substring(0, $text.IndexOf("`n"))
      if ($first -cnotmatch "^# agentspace lease $([regex]::Escape($id)); .+$") { return "The first line is '$first'." }
      $true
    }
    $keyFwd = (Join-Path (Get-CcLeaseDir $lab $id) 'id_ed25519').Replace('\', '/').Replace('%', '%%')
    $certFwd = (Join-Path (Get-CcLeaseDir $lab $id) 'id_ed25519-cert.pub').Replace('\', '/').Replace('%', '%%')
    $expectedRest = @(
      "Host $alias", "  User $user", "  IdentityFile `"$keyFwd`"", "  CertificateFile `"$certFwd`"", '  IdentitiesOnly yes',
      '  HostKeyAlias agentspace', '  StrictHostKeyChecking yes', "  ProxyCommand $($script:CcProxy.Replace('%', '%%'))",
      '  ServerAliveInterval 30', '  ServerAliveCountMax 4', '  RemoteForward 127.0.0.1:8811 127.0.0.1:8811',
      '  RemoteForward 127.0.0.1:53147 127.0.0.1:53147') -join "`n"
    Test-Cc "$aliasDoc`: Every line after the first is exactly the block state.md gives, indented by two spaces (no ClearAllForwardings)" {
      if (-not $text) { return 'There is no alias file.' }
      $rest = $text.Substring($text.IndexOf("`n") + 1)
      if ($rest -cne ($expectedRest + "`n")) { return "The lines after the first are not as documented: '$($rest.Replace("`n", '|'))'." }
      $true
    }
    Test-Cc "$aliasDoc`: The key and certificate paths are the lease's own files, and those files exist" {
      if (-not (Test-Path -LiteralPath (Join-Path (Get-CcLeaseDir $lab $id) 'id_ed25519')) -or -not (Test-Path -LiteralPath (Join-Path (Get-CcLeaseDir $lab $id) 'id_ed25519-cert.pub'))) { return 'The key or certificate is missing.' }
      $true
    }

    # What ssh makes of the file.
    $g = if ($bytes) { Get-CcSshG $aliasPath $alias } else { @() }
    $gt = $g -join "`n"
    Test-Cc "$aliasDoc`: ssh -G reads the alias file: user, identity file, certificate file, identities only" {
      if (-not $bytes) { return 'There is no alias file.' }
      if ($gt -cnotmatch "(?m)^user $([regex]::Escape($user))\s*`$") { return "The user is not ssh.user, '$user'." }
      if ($gt -inotmatch "(?m)^identityfile .*$([regex]::Escape($keyFwd))\s*$") { return 'The identity file is not the lease key.' }
      if ($gt -inotmatch "(?m)^certificatefile .*$([regex]::Escape($certFwd))\s*$") { return 'The certificate file is not the lease certificate.' }
      if ($gt -cnotmatch '(?m)^identitiesonly yes\s*$') { return 'identitiesonly is not yes.' }
      $true
    }
    Test-Cc "$aliasDoc`: ssh -G reads the host key settings, the keep-alive settings and both remote forwards" {
      if (-not $bytes) { return 'There is no alias file.' }
      if ($gt -cnotmatch '(?m)^hostkeyalias agentspace\s*$') { return 'hostkeyalias is not agentspace.' }
      if ($gt -cnotmatch '(?m)^stricthostkeychecking true\s*$') { return 'stricthostkeychecking is not true.' }
      if ($gt -cnotmatch '(?m)^serveraliveinterval 30\s*$' -or $gt -cnotmatch '(?m)^serveralivecountmax 4\s*$') { return 'The keep-alive settings are not 30 and 4.' }
      if ($gt -cnotmatch '(?m)^remoteforward \[127\.0\.0\.1\]:8811 \[127\.0\.0\.1\]:8811\s*$' -or $gt -cnotmatch '(?m)^remoteforward \[127\.0\.0\.1\]:53147 \[127\.0\.0\.1\]:53147\s*$') { return 'The two remote forwards are not both there.' }
      $true
    }
    Test-Cc "lease-api.md#environment: AGENTSPACE_PROXY_COMMAND replaces the transport whole, and ssh -G shows it" {
      if (-not $bytes) { return 'There is no alias file.' }
      if ($gt -cnotmatch "(?m)^proxycommand $([regex]::Escape($script:CcProxy))\s*$") { return 'The proxy command is not the override.' }
      $true
    }

    # A literal percent sign is written doubled, because ssh expands `%` tokens in a ProxyCommand.
    $pct = 'agentspace-conformance-proxy --pct 100%'
    $cp = Invoke-CcConfigCase $lab $null 'cfg' @{ AGENTSPACE_PROXY_COMMAND = $pct }
    $pAlias = if ($cp.Lease.Ok) { [string]$cp.Lease.Doc['ssh']['host_alias'] } else { '' }
    $pBytes = Get-CcBytes (Get-CcAliasPath $lab $pAlias)
    $pText = if ($pBytes) { $utf8.GetString($pBytes) } else { '' }
    Test-Cc "$aliasDoc`: A percent sign in the ProxyCommand is written as two, and nothing else of it is changed" {
      if (-not $cp.Lease.Ok) { return "The create gave $(Format-CcRun $cp.Lease.Run)" }
      if (-not $pText.Contains("`n  ProxyCommand agentspace-conformance-proxy --pct 100%%`n")) { return 'The ProxyCommand line is not the override with % doubled.' }
      $pg = (Get-CcSshG (Get-CcAliasPath $lab $pAlias) $pAlias) -join "`n"
      # ssh -G prints the command as written; ssh expands %% to % only when it runs the command.
      if ($pg -cnotmatch '(?m)^proxycommand agentspace-conformance-proxy --pct 100%%\s*$') { return 'ssh -G does not read the ProxyCommand as one line with the doubled sign.' }
      $true
    }

    # The default transport (state.md, "The alias file"; D8). With AGENTSPACE_PROXY_COMMAND removed, the
    # program forms its own line, which this case checks only as far as D8 makes it visible from
    # outside: the full path of a program that exists, in double quotes, then one space and the alias
    # written literally (never ssh's %n), with the system folder's cmd.exe /d /c in front when, and only
    # when, the program is a .cmd or .bat. Where the program sits is not checked.
    # The case seeds no instance-ID cache, and a conforming build needs none: `create` MUST NOT depend
    # on it (D21; driver.md, "How it is reached"). That MUST is asserted by
    # test-unmet-default-transport-no-cache.ps1, because the prototype misses it in a checkout with no
    # .instance-id of its own, such as a worktree. So when this create fails with a valid `internal`
    # error, what it leaves is still checked, and the default line's result is SKIPPED, naming that
    # file: the run is incomplete, never passed. Any other failure of the create is a FAIL.
    Reset-CcSshDir $lab
    $leasesDirD = Join-Path $lab.UserProfile '.agentspace\leases'
    $snapD = "$((Get-CcFileNames $leasesDirD) -join '|');$((Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')) -join '|');$(Get-CcStateCanonical $lab)"
    $dflt = New-CcReadyLease -Lab $lab -Project 'cfg' -EnvVars @{ AGENTSPACE_PROXY_COMMAND = $null }
    $snapAfterD = "$((Get-CcFileNames $leasesDirD) -join '|');$((Get-CcFileNames (Join-Path $lab.SshDir 'agentspace.d')) -join '|');$(Get-CcStateCanonical $lab)"
    $dName = "$aliasDoc`: The default ProxyCommand is the full path of a program that exists, then the alias written literally, through the system folder's cmd.exe /d /c when and only when the program is a .cmd or .bat"
    if (-not $dflt.Ok -and (Test-CcFailure $dflt.Run 1 'internal')) {
      Test-Cc "lease-api.md#concurrency-and-atomicity: A create that fails under the default transport leaves no lease folder, no alias file and no record" {
        if ($snapAfterD -cne $snapD) { return 'The lease folders, the alias folder or the state file changed.' }
        $true
      }
      Write-ConformanceResult -Name $dName -Status SKIPPED `
        -Reason 'Create failed with internal under the default transport, so there is no default line to read. That create MUST succeed with no instance-ID cache, which test-unmet-default-transport-no-cache.ps1 asserts.'
    }
    else {
      $dAlias = if ($dflt.Doc) { [string]$dflt.Doc['ssh']['host_alias'] } else { '' }
      $dBytes = Get-CcBytes (Get-CcAliasPath $lab $dAlias)
      $dText = if ($dBytes) { $utf8.GetString($dBytes) } else { '' }
      Test-Cc $dName {
        if (-not $dflt.Ok) { return "The create gave $(Format-CcRun $dflt.Run)" }
        if (-not $dBytes) { return 'There is no alias file.' }
        $problems = Get-CcDefaultProxyProblems -AliasText $dText -Alias $dAlias
        if ($problems.Count -gt 0) { return ($problems -join '; ') + '.' }
        $dg = (Get-CcSshG (Get-CcAliasPath $lab $dAlias) $dAlias) -join "`n"
        if ($dg -cnotmatch "(?m)^proxycommand .+ $([regex]::Escape($dAlias))\s*$") { return 'ssh -G does not show a command ending with the alias.' }
        $true
      }
    }
  }
}
catch {
  Write-ConformanceResult -Name 'No unexpected error in test-lease-alias-file' -Status FAIL -Reason ("It threw: " + ($_.Exception.Message -replace '\s+', ' '))
}
finally {
  Remove-ConformanceHome -ConformanceHome $lab
}
Exit-ConformanceRun

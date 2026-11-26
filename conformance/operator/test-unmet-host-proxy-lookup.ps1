#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the host the ProxyCommand finds from its one argument (docs/operator.md, "From
# the ProxyCommand", D49; "Which checks show it", the row for the ProxyCommand's host), run through
# the test seam (docs/operator.md, "The test seam"). Story E0.07-T04-S02 (#0156). It shows, for each
# kind of argument, which instance ID the stand-in `aws` is asked about and tunnelled to:
#   - no argument: the box; `agentspace`: the box, since no lease alias is that;
#   - a host alias `agentspace-host.<project>`: that project's record, whatever its state (`ready`,
#     `creating` or `failed`), `agentspace-host.swimcoach` included; a rest that is not a normalized
#     project name, or a project with no record, fails as a failed `up` does in caller mode;
#   - a lease alias: the record in the lease state file that is the alias's (the alias built from it, or a
#     stored `ready` record's `ssh.host_alias`, exactly), then its project's host if that lease is
#     `ready`; otherwise the box (a project whose host is not `ready`, a project with no record, an
#     alias no lease has, a different case, no lease state file, a lease state file that cannot be read
#     or is not valid for an argument that is not of lease alias form); from v0.11 the lease's expiry is
#     checked, and a lease that has expired, and a lease-form argument while the lease state file cannot
#     be read or is not valid, are refused with exit 1 and no tunnel (#0229, pin P21; #0237). The refusal's
#     whole shape is test-unmet-proxy-not-ready.ps1's, and this file shows only that each is refused;
#   - the ProxyCommand never writes the lease state file or takes its lock;
#   - a host registry that is not valid fails the connection with the caller's line, for each kind of
#     argument, and starts no tunnel.
#
# The records hold their instance IDs, so each case is shown by the ID the tunnel is started for.
# The stand-in `aws` answers `describe-instances` with a running instance launched long ago and the
# tunnel with a marker on stdout, and logs every call's full arguments. AGENTSPACE_NESTED is set, so
# `up` pushes nothing, and ssh and scp are tripwires that no step may call. The push's stamp per host
# is test-unmet-host-proxy-stamp.ps1's; the ID a record is given is test-unmet-host-proxy-instance-id.ps1's.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and
# removes every AWS credential, and the check confirms first that `aws sts get-caller-identity` fails
# under that environment for every AWS CLI the operator can run (SKIPPED if not). Every ID is made up.
# A failure reason never quotes the program's output.
#
# The program under test is the one AGENTSPACE_SSH_PROXY_CMD names; unset, it takes its default from
# conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-host-proxy-lookup.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'operator.md#from-the-proxycommand'
$regSec = 'operator.md#the-host-registry-as-the-operator-uses-it'

# The problems of a run the ProxyCommand refused from v0.11 (operator.md, "A lease that is not ready (#0229)"):
# exit 1, nothing on stdout, no call the stand-in aws records, no tunnel, no ssh or scp, and a last line on
# stderr that starts `agentspace: ` and holds each of $Words. The refusal's full shape is
# test-unmet-proxy-not-ready.ps1's.
function Get-LkRefusedProblems($Run, $S, [string[]]$Words) {
  $p = New-HpProblems
  if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
  if ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
  if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
  $a = Get-HpAsked $S
  if ($a.Raw.Count -gt 0) { $p.Add('The stand-in aws was called') }
  if ($a.Tunnels.Count -gt 0) { $p.Add('A tunnel was started') }
  if ((Get-HpSshCallCount $S) -gt 0 -or (Get-StandInCallCount -Log (Join-Path $S.Dir 'scp-tripwire-calls.log')) -gt 0) { $p.Add('ssh or scp was started') }
  $lines = @($Run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
  if ($lines.Count -eq 0) { $p.Add('Stderr is empty'); return , $p }
  $last = $lines[-1].Trim()
  if (-not $last.StartsWith('agentspace: ', [StringComparison]::Ordinal)) { $p.Add('The last line on stderr does not start with "agentspace: "') }
  foreach ($w in $Words) { if ($last -notmatch [regex]::Escape($w)) { $p.Add("The last line on stderr does not hold '$w'") } }
  , $p
}

$leaseOther = '1111111a-1111-4111-8111-11111111111a'
$leaseThird = '2222222b-2222-4222-8222-22222222222b'
$leaseGhost = '3333333c-3333-4333-8333-33333333333c'
$leaseExpired = '4444444d-4444-4444-8444-44444444444d'
$aliasOther = 'agentspace-other-1a2b3c4d'
$aliasThird = 'agentspace-third-1a2b3c4e'
$aliasGhost = 'agentspace-ghost-1a2b3c4f'
$aliasExpired = 'agentspace-other-1a2b3c50'

$h = New-ConformanceHome
try {
  $name0 = "${sec}: the ProxyCommand finds its host from its argument"
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program'
  } else {
    $S = New-HpStandIns $h
    $validRegistry = @(
      (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox),
      (New-HpHostRecord 'other' 'ready' $HpIdOther),
      (New-HpHostRecord 'third' 'creating' $HpIdThird),
      (New-HpHostRecord 'broken' 'failed' $HpIdFailed))
    $validLeases = @(
      (New-HpLeaseRecord $leaseOther 'other' $aliasOther),
      (New-HpLeaseRecord $leaseThird 'third' $aliasThird),
      (New-HpLeaseRecord $leaseGhost 'ghost' $aliasGhost),
      (New-HpLeaseRecord $leaseExpired 'other' $aliasExpired '2026-01-02T00:00:00Z'))
    Set-HpRegistry $h $validRegistry
    Set-HpLeases $h $validLeases
    $leasesPath = Get-HpLeasesPath $h
    $leasesHash = Get-HpBytesHash $leasesPath
    $leasesLock = Join-Path $h.StateDir 'leases.lock'

    # ---- Each argument and the instance it reaches ----
    $reach = @(
      @{ N = 'no argument reaches the box (step 1)'; A = @(); Want = $HpIdBox },
      @{ N = 'the argument agentspace matches no lease and reaches the box'; A = @('agentspace'); Want = $HpIdBox },
      @{ N = 'a host alias reaches that project''s host, agentspace-host.other'; A = @('agentspace-host.other'); Want = $HpIdOther },
      @{ N = 'a host alias reaches a host that is still creating, agentspace-host.third'; A = @('agentspace-host.third'); Want = $HpIdThird },
      @{ N = 'a host alias reaches a host that failed, agentspace-host.broken'; A = @('agentspace-host.broken'); Want = $HpIdFailed },
      @{ N = 'agentspace-host.swimcoach reaches the box''s record'; A = @('agentspace-host.swimcoach'); Want = $HpIdBox },
      @{ N = 'a lease alias of a ready host reaches that host'; A = @($aliasOther); Want = $HpIdOther },
      @{ N = 'a lease alias of a host that is not ready reaches the box'; A = @($aliasThird); Want = $HpIdBox },
      @{ N = 'a lease alias of a project with no host record reaches the box'; A = @($aliasGhost); Want = $HpIdBox },
      @{ N = 'an alias no lease has reaches the box'; A = @('agentspace-nope-12345678'); Want = $HpIdBox },
      @{ N = 'a lease alias in another case matches no lease (compared exactly) and reaches the box'; A = @($aliasOther.ToUpperInvariant()); Want = $HpIdBox }
    )
    foreach ($c in $reach) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S $c.A
      $p = Get-HpReachedProblems $r $S $c.Want
      Write-HpProblems "${sec}: $($c.N)" $p
    }

    # From v0.11 the lease's expiry is checked: a ready lease past its expires_at is expired, and refused.
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasExpired)
    Write-HpProblems "${sec}: A lease alias whose lease has expired is refused, exit 1, a line naming expired, no tunnel (the expiry is checked from v0.11)" (Get-LkRefusedProblems $r $S @('expired', 'no connection was opened', $aliasExpired))

    $p = New-HpProblems
    if ((Get-HpBytesHash $leasesPath) -cne $leasesHash) { $p.Add('The lease state file changed') }
    if (Test-Path -LiteralPath $leasesLock) { $p.Add('The lease state lock file was made, so the lock was taken') }
    Write-HpProblems "${sec}: The ProxyCommand never writes the lease state file or takes its lock (step 3)" $p

    # ---- A host alias that fails ----
    $bad = @(
      @{ N = 'a project with no record, agentspace-host.nobody'; A = 'agentspace-host.nobody' },
      @{ N = 'a rest that is not lowercase, agentspace-host.Other'; A = 'agentspace-host.Other' },
      @{ N = 'an empty rest, agentspace-host.'; A = 'agentspace-host.' },
      @{ N = 'a rest that starts with a dash, agentspace-host.-other'; A = 'agentspace-host.-other' },
      @{ N = 'a rest that holds a dot, agentspace-host.other.x'; A = 'agentspace-host.other.x' }
    )
    foreach ($c in $bad) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($c.A)
      Write-HpProblems "${sec}: A host alias with $($c.N) fails as a failed up does, exit 1, no tunnel" (Get-HpRefusedProblems $r $S)
    }

    # ---- A lease state file that cannot be read or is not valid (pin P21, from v0.11): a lease-form argument
    # is refused, and an argument that is not of lease alias form still reaches the box ----
    $badLeases = @(
      @{ N = 'is not JSON'; Text = 'this is not json' },
      @{ N = 'has version 1'; Text = '{"version":1,"leases":[' + (New-HpLeaseRecord $leaseOther 'other' $aliasOther) + ']}' },
      @{ N = 'holds two records with one id'; Text = '{"version":0,"leases":[' + (New-HpLeaseRecord $leaseOther 'other' $aliasOther) + ',' + (New-HpLeaseRecord $leaseOther 'other' $aliasOther) + ']}' },
      @{ N = 'holds a record that does not fit its schema'; Text = '{"version":0,"leases":[' + (New-HpLeaseRecord $leaseOther 'other' $aliasOther).Replace('"state":"ready"', '"state":"weird"') + ']}' }
    )
    foreach ($c in $badLeases) {
      Remove-HpPath $h $leasesPath
      Set-HpFile $h $leasesPath ($c.Text + "`n")
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($aliasOther)
      Write-HpProblems "${sec}: A lease state file that $($c.N) refuses a lease alias, whatever the file names: exit 1, the lease records could not be read, no tunnel (P21)" (Get-LkRefusedProblems $r $S @('records could not be read', 'no connection was opened'))
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @('agentspace')
      Write-HpProblems "${sec}: A lease state file that $($c.N) still sends an argument that is not of lease alias form, agentspace, to the box (the box's ID)" (Get-HpReachedProblems $r $S $HpIdBox)
    }
    Remove-HpPath $h $leasesPath
    New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $h -Path $leasesPath) | Out-Null
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasOther)
    Write-HpProblems "${sec}: A folder at the lease state file's path refuses a lease alias: exit 1, the lease records could not be read, no tunnel (P21)" (Get-LkRefusedProblems $r $S @('records could not be read', 'no connection was opened'))
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace')
    Write-HpProblems "${sec}: A folder at the lease state file's path still sends an argument that is not of lease alias form, agentspace, to the box" (Get-HpReachedProblems $r $S $HpIdBox)
    Remove-HpPath $h $leasesPath

    # No lease state file at all reads as the box (and, with a host alias, still finds the host).
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasOther)
    Write-HpProblems "${sec}: No lease state file reads as the box for a lease alias (a missing file is not refused)" (Get-HpReachedProblems $r $S $HpIdBox)
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace-host.other')
    Write-HpProblems "${sec}: No lease state file does not stop a host alias from reaching its host" (Get-HpReachedProblems $r $S $HpIdOther)

    # With AGENTSPACE_DIAGNOSTIC set, the ProxyCommand prints a line naming the lease state file.
    Set-HpFile $h $leasesPath "this is not json`n"
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($aliasOther) -Extra @{ AGENTSPACE_DIAGNOSTIC = '1' }
    $p = Get-LkRefusedProblems $r $S @('records could not be read', 'no connection was opened')
    $lines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
    if (@($lines | Select-Object -SkipLast 1 | Where-Object { $_ -match 'leases\.json' }).Count -eq 0) { $p.Add('With AGENTSPACE_DIAGNOSTIC set, no line before the refusal names the lease state file') }
    if ($lines.Count -gt 0 -and $lines[-1] -match 'leases\.json') { $p.Add('The refusal''s own line names the file') }
    Write-HpProblems "${sec}: With AGENTSPACE_DIAGNOSTIC set, a lease state file that is not valid is named on stderr before the refusal of a lease alias, whose line is the last (P21)" $p
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @('agentspace') -Extra @{ AGENTSPACE_DIAGNOSTIC = '1' }
    $p = Get-HpReachedProblems $r $S $HpIdBox
    if ($r.Stderr -notmatch 'leases\.json') { $p.Add('With AGENTSPACE_DIAGNOSTIC set, stderr names no line about the lease state file') }
    Write-HpProblems "${sec}: With AGENTSPACE_DIAGNOSTIC set, a lease state file that is not valid is named on stderr, and an argument that is not of lease alias form still reaches the box" $p
    Remove-HpPath $h $leasesPath
    Set-HpLeases $h $validLeases

    # ---- A host registry that is not valid fails the connection ----
    $e = (New-HpHostRecord 'swimcoach' 'ready' $HpIdBox)
    $badRegistries = @(
      @{ N = 'is not JSON'; Text = 'this is not json' },
      @{ N = 'has version 1'; Text = '{"version":1,"hosts":[' + $e + ']}' },
      @{ N = 'holds no record whose stack is agentbox'; Text = '{"version":0,"hosts":[' + (New-HpHostRecord 'other' 'ready' $HpIdOther) + ']}' },
      @{ N = 'holds two records for one project'; Text = '{"version":0,"hosts":[' + $e + ',' + (New-HpHostRecord 'other' 'ready' $HpIdOther) + ',' + (New-HpHostRecord 'other' 'creating') + ']}' },
      @{ N = 'holds a record whose stack names another project'; Text = '{"version":0,"hosts":[' + $e + ',{"project":"other","stack":"agentspace-host-third","state":"ready"}]}' },
      @{ N = 'holds an instance ID that does not match its pattern'; Text = '{"version":0,"hosts":[' + $e.Replace($HpIdBox, 'i-XYZ') + ']}' }
    )
    foreach ($c in $badRegistries) {
      Remove-HpPath $h (Get-HpRegistryPath $h)
      Set-HpFile $h (Get-HpRegistryPath $h) ($c.Text + "`n")
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @()
      Write-HpProblems "${regSec}: A host registry that $($c.N) fails the connection with the caller's line, exit 1, no tunnel (no argument)" (Get-HpRefusedProblems $r $S)
    }
    Remove-HpPath $h (Get-HpRegistryPath $h)
    Set-HpFile $h (Get-HpRegistryPath $h) "this is not json`n"
    foreach ($a in 'agentspace-host.other', $aliasOther) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($a)
      $kind = if ($a -like 'agentspace-host.*') { 'a host alias' } else { 'a lease alias' }
      Write-HpProblems "${regSec}: A host registry that is not JSON fails the connection for $kind, exit 1, no tunnel" (Get-HpRefusedProblems $r $S)
    }
    Remove-HpPath $h (Get-HpRegistryPath $h)
    New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $h -Path (Get-HpRegistryPath $h)) | Out-Null
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @()
    Write-HpProblems "${regSec}: A folder at the host registry's path fails the connection, exit 1, no tunnel" (Get-HpRefusedProblems $r $S)
    Remove-HpPath $h (Get-HpRegistryPath $h)
    # The registry is never read as missing and never repaired.
    $p = New-HpProblems
    Set-HpFile $h (Get-HpRegistryPath $h) "this is not json`n"
    $before = Get-HpBytesHash (Get-HpRegistryPath $h)
    $r = Invoke-HpProxy $h $S @()
    if ((Get-HpBytesHash (Get-HpRegistryPath $h)) -cne $before) { $p.Add('The invalid registry was changed') }
    Write-HpProblems "${regSec}: A host registry that is not valid is never repaired or replaced by the ProxyCommand" $p
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-host-proxy-lookup.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

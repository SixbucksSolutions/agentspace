#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Conformance check of the ProxyCommand's refusal of a lease alias whose lease is not `ready`, and of a
# lease-form argument while the lease state file cannot be read or is not valid (docs/operator.md, "From the
# ProxyCommand", step 3; "A lease that is not ready (#0229)"; "What a caller sees"; "Which checks show it",
# v0.11's table; docs/conformance.md, the ProxyCommand's refusal), run through the test seam (docs/operator.md,
# "The test seam"). Story E0.11-T02-S04 (#0237), D89; pins P21, P22, P23 and P27 (architect, 2026-11-25, for
# v0.11-T01). It shows, with the stand-in `aws` that logs every call, tripwires for ssh and scp, a seeded host
# registry and a seeded lease state file:
#   - the alias of a lease that is `released` (with and without an expires_at), `expired` (a stored `ready`
#     record past its expires_at, at the second it was seeded, and found by its stored ssh.host_alias),
#     `unsatisfiable` (no_capacity and plan_too_large) or `pending`, each of a project whose host is `ready`,
#     refused: exit 1, one line on stderr that starts `agentspace: `, names the state and no other and says no
#     connection was opened, nothing on stdout, no call the stand-in records, no tunnel, no ssh or scp, no
#     file in the home changed, and a line that holds no lease ID beyond the alias's 8 hex digits, no path and
#     no backend text; AGENTSPACE_DIAGNOSTIC adds no line to such a refusal; the refusal comes before the host
#     registry is read (a registry that is not valid is neither read nor repaired);
#   - a `released` lease of a project with no host record refused the same way;
#   - a `ready` lease still connecting (found by the alias built from its record and by its stored alias), a
#     `ready` lease of a host that is not `ready` or has no record reaching the box, and an alias that is no
#     record's, and a missing lease state file, reaching the box; the ProxyCommand never writes the lease state
#     file or takes its lock;
#   - a lease state file that cannot be read or is not valid (not JSON, version 1, two records with one id, a
#     record that does not fit its schema, a folder at the path) refusing an argument of lease alias form with
#     the line saying the lease records could not be read, and sending `agentspace`, an upper case hex alias, one
#     with 7 hex digits and one with 9 to the box; with AGENTSPACE_DIAGNOSTIC set, the line naming the file
#     comes before the refusal, which stays the last, and no retry line is printed (P21, P23, P27);
#   - the sharing-violation retry (P23, P27): a lease state file the case holds open without sharing, past the
#     read's bound, refusing a lease-form argument with two retry lines, and sending `agentspace`, an upper case
#     hex alias and one with 7 hex digits to the box after the same two; one the case releases on the first
#     retry line to reach the ProxyCommand's stderr, never on a timer, connecting after one or two;
#   - two records that are one alias's (P22): the first `ready` one is the alias's lease, a stored `ready`
#     record past its expires_at is skipped and is named `expired` when it is the last, and two that are not
#     `ready` are refused with the later one's state;
#   - a `ready` lease made by the build's `agentspace lease create`, connecting, then released by its
#     `agentspace lease release`, then refused as `released` with no call the stand-in records, in that order
#     and never at once; a person's `up -Alias <alias>` for that alias not refused.
#
# The records hold made-up instance IDs, so each case that reaches a host is shown by the ID the tunnel is
# started for. A refusal's run is made with AGENTSPACE_NESTED unset and a made-up token fixture, so a build that
# pushed before refusing would start the ssh tripwire. No case swaps, replaces or releases a file on a timer, and
# every run ends within its bound, or the case fails and ends it. A failure reason never quotes the program's
# output.
#
# SAFETY. The harness points USERPROFILE, HOME, LOCALAPPDATA and APPDATA into the throwaway home and removes every
# AWS credential, and the check confirms first that `aws sts get-caller-identity` fails under that environment for
# every AWS CLI the operator can run (SKIPPED if not). The release flow's lease is made with the throwaway CA in a
# home of its own (SKIPPED when no CA could be made). Every ID is made up, and the token is a made-up value.
#
# The programs under test are the ones AGENTSPACE_SSH_PROXY_CMD, AGENTSPACE_CMD and AGENTSPACE_OPERATOR_CMD name;
# unset, each takes its default from conformance/defaults.ps1 (docs/conformance.md, "Naming the program under test").
#
# Run as: pwsh -NoProfile -File conformance/operator/test-unmet-proxy-not-ready.ps1
. (Join-Path $PSScriptRoot 'host-proxy-common.ps1')

$sec = 'operator.md#a-lease-that-is-not-ready-0229'
$fromSec = 'operator.md#from-the-proxycommand'
$RetryLine = 'agentspace: The lease records were held by another program; trying again.'
$StateWords = 'released', 'expired', 'unsatisfiable', 'pending'
$past = '2026-01-02T00:00:00Z'
$future = '2099-01-01T00:00:00Z'

# ---- Made-up lease records ----
function New-NrId([int]$N) { 'b{0:x7}-5555-4555-8555-555555555555' -f $N }
function Get-NrAlias([string]$Project, [string]$Id) { "agentspace-$Project-$($Id.Substring(0, 8))" }
# A record as state.md's table gives it for its state (schemas/state/leases.state.schema.json); Set-NrLeases checks it.
function New-NrRecord([string]$Id, [string]$Project, [string]$State, [string]$Expires = '', [string]$StoredAlias = '', [string]$Reason = 'no_capacity') {
  $size = if ($Reason -ceq 'plan_too_large') { 'xl' } else { 'm' }
  $t = '"id":"' + $Id + '","project":"' + $Project + '","state":"' + $State + '","platform":"linux/arm64","profile":"agent-linux@3","size":"' + $size + '","ttl_seconds":28800,"created_at":"2026-01-01T00:00:00Z"'
  if ($State -ceq 'ready') {
    $stored = if ($StoredAlias) { $StoredAlias } else { Get-NrAlias $Project $Id }
    $t += ',"expires_at":"' + $Expires + '","ssh":{"host_alias":"' + $stored + '","user":"ubuntu"},"driver":"ec2"'
  } elseif ($State -ceq 'released') {
    if ($Expires) { $t += ',"expires_at":"' + $Expires + '"' }
    $t += ',"released_at":"2026-01-01T01:00:00Z","driver":"ec2"'
  } elseif ($State -ceq 'unsatisfiable' -and $Reason -ceq 'plan_too_large') {
    $subs = (1..32 | ForEach-Object { '{"name":"s' + $_ + '","model":"Sonnet","effort":"medium"}' }) -join ','
    $t += ',"sized_for":{"plan":"conf","turn":"T01","agents":33},"reasons":[{"code":"plan_too_large","message":"The busiest turn has 33 agents at once; the largest size holds 32."}],"driver":"ec2"'
    $t += ',"plan":{"name":"conf","turns":[{"name":"T01","stories":[{"name":"S01","subagents":[' + $subs + ']}]}]}'
  } elseif ($State -ceq 'unsatisfiable') {
    $t += ',"reasons":[{"code":"no_capacity","message":"The project has no host."}],"driver":"ec2"'
  } else {
    $t += ',"driver":"ec2"'
  }
  '{' + $t + '}'
}
function New-NrSpecRecord([string]$Spec, [string]$Id) {
  if ($Spec -ceq 'expired') { New-NrRecord $Id 'other' 'ready' $past } elseif ($Spec -ceq 'ready') { New-NrRecord $Id 'other' 'ready' $future } else { New-NrRecord $Id 'other' $Spec }
}
function Set-NrLeases($H, [string[]]$Records) {
  $text = '{"version":0,"leases":[' + ($Records -join ',') + ']}'
  if (-not (Test-ConformanceSchema -Json $text -Schema 'state/leases.state.schema.json')) { throw 'A seeded lease state file does not validate against its schema' }
  Remove-HpPath $H (Get-HpLeasesPath $H)
  Set-HpFile $H (Get-HpLeasesPath $H) ($text + "`n")
}

# ---- What a run changed, and what its refusal says ----
# Every file and folder under the home's root but the stand-ins' own, by size, time and hash, so that a write, a
# rewrite with the same bytes, a lock file or a new folder shows. A file the case holds open is never read here.
function Get-NrSnapshot($H) {
  $root = [IO.Path]::GetFullPath([string]$H.Root).TrimEnd('\') + '\'
  $skip = $root + 'standins\'
  $rows = [System.Collections.Generic.List[string]]::new()
  foreach ($i in Get-ChildItem -LiteralPath $H.Root -Recurse -Force) {
    if (($i.FullName + '\').StartsWith($skip, [StringComparison]::OrdinalIgnoreCase)) { continue }
    $rel = $i.FullName.Substring($root.Length)
    if ($i.PSIsContainer) { $rows.Add("D $rel") } else { $rows.Add("F $rel $($i.Length) $($i.LastWriteTimeUtc.Ticks) $(Get-HpBytesHash $i.FullName)") }
  }
  $rows.Sort([StringComparer]::Ordinal)
  , $rows.ToArray()
}
function Get-NrSnapshotProblems($Before, $H) {
  $p = New-HpProblems
  $after = Get-NrSnapshot $H
  $d = @(Compare-Object -ReferenceObject $Before -DifferenceObject $after -CaseSensitive | ForEach-Object { ($_.InputObject -split ' ')[1] } | Select-Object -Unique)
  if ($d.Count -gt 0) { $p.Add("A file or folder in the home changed: $(($d | Select-Object -First 3) -join ', ')") }
  , $p
}
function Get-NrRetryCount($Run) { @($Run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ceq $RetryLine }).Count }
# The refusal's line (operator.md, "The refusal's line"). $State is the lease's state, or '' for the line that says
# the lease records could not be read.
function Get-NrLineProblems([string]$Line, [string]$State, [string]$Alias, $H) {
  $p = New-HpProblems
  if (-not $Line.StartsWith('agentspace: ', [StringComparison]::Ordinal)) { $p.Add('The line does not start with "agentspace: "') }
  if ($Line -notmatch 'no connection was opened') { $p.Add('The line does not say that no connection was opened') }
  if (-not $Line.Contains($Alias)) { $p.Add('The line does not name the alias') }
  $bare = $Line.Replace($Alias, '')
  if ($State) {
    if ($bare -notmatch "\b$State\b") { $p.Add("The line does not name the state $State") }
    foreach ($w in $StateWords) { if ($w -cne $State -and $bare -match "\b$w\b") { $p.Add("The line names another state, $w") } }
  } else {
    if ($bare -notmatch 'lease records could not be read') { $p.Add('The line does not say that the lease records could not be read') }
    foreach ($w in $StateWords) { if ($bare -match "\b$w\b") { $p.Add("The line names a state, $w") } }
  }
  if ($Line -match '(?i)[0-9a-f]{8}-[0-9a-f]{4}-') { $p.Add('The line holds a lease ID') }
  if ($bare -match '(?i)[0-9a-f]{9,}') { $p.Add('The line holds more than 8 hex digits of an ID') }
  if ($Line -match '[A-Za-z]:[\\/]|\\\\|\.json|\.lock|leases' -or $Line.Contains([string]$H.Root) -or $Line.Contains([string]$H.UserProfile)) { $p.Add('The line holds a path or a file name') }
  if ($Line -match '(?i)\b(EC2|AWS|SSM|CloudFormation|Session Manager)\b|agentbox|agentspace-host|us-west-|m9g|\bi-[0-9a-f]{8,}|\b(running|stopped|stopping|terminated|shutting-down)\b') { $p.Add('The line holds backend text') }
  , $p
}
# A refusal run (exit 1, nothing on stdout, one line on stderr, or with -FileLine the retry lines and one line
# naming the lease state file before it, no AWS call, no tunnel, no ssh or scp, no change to the home). $Before is
# a snapshot taken before the run, or $null while the case still holds a file open.
function Get-NrRefusalProblems($Run, $S, $H, $Before, [string]$State, [string]$Alias, [switch]$FileLine) {
  $p = New-HpProblems
  if ($Run.TimedOut) { $p.Add('It timed out'); return , $p }
  if ($Run.ExitCode -ne 1) { $p.Add("Exit $($Run.ExitCode), not 1") }
  if ($Run.Stdout.Length -gt 0) { $p.Add('Stdout is not empty') }
  $lines = @($Run.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | ForEach-Object { $_.Trim() })
  if ($lines.Count -eq 0) { $p.Add('Stderr is empty'); return , $p }
  $early = @($lines | Select-Object -SkipLast 1)
  if ($FileLine) {
    $rest = @($early | Where-Object { $_ -cne $RetryLine })
    if ($rest.Count -ne 1 -or $rest[0] -notmatch 'leases\.json') { $p.Add('Before the refusal there is not exactly one line naming the lease state file, besides the retry lines') }
  } elseif ($early.Count -gt 0) { $p.Add("Stderr holds $($lines.Count) lines, not one") }
  foreach ($x in (Get-NrLineProblems $lines[-1] $State $Alias $H)) { $p.Add($x) }
  $a = Get-HpAsked $S
  if ($a.Raw.Count -gt 0) { $p.Add('The stand-in aws was called') }
  if ($a.Tunnels.Count -gt 0) { $p.Add('A tunnel was started') }
  if ((Get-HpSshCallCount $S) -gt 0 -or (Get-StandInCallCount -Log (Join-Path $S.Dir 'scp-tripwire-calls.log')) -gt 0) { $p.Add('ssh or scp was started') }
  if ($null -ne $Before) { foreach ($x in (Get-NrSnapshotProblems $Before $H)) { $p.Add($x) } }
  , $p
}
function Invoke-NrRefused($H, $S, [string]$Alias, [hashtable]$Extra = @{}, [int]$Timeout = 30) {
  Clear-HpLogs $H $S
  Invoke-HpProxy $H $S @($Alias) -Extra $Extra -Push -Timeout $Timeout
}
# Runs the ProxyCommand with AGENTSPACE_DIAGNOSTIC set and the case's hold on the lease state file open, reads its
# stderr line by line, and disposes of the hold when the first retry line arrives. Bounded: a run that has not ended
# in $TimeoutSeconds is ended, by its own process, and reported TimedOut.
function Invoke-NrHeld($H, $S, [string]$Alias, $Hold, [int]$TimeoutSeconds = 10) {
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $H -Override @{ AGENTSPACE_AWS_CMD = $S.Aws; AGENTSPACE_SSH_CMD = $S.Ssh; AGENTSPACE_SCP_CMD = $S.Scp; AGENTSPACE_NESTED = '1'; AGENTSPACE_DIAGNOSTIC = '1' }
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $program
  $psi.ArgumentList.Add($Alias)
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  foreach ($k in $childEnv.Keys) { if ($null -eq $childEnv[$k]) { [void]$psi.Environment.Remove($k) } else { $psi.Environment[$k] = [string]$childEnv[$k] } }
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  $released = $false
  $timedOut = $false
  $lines = [System.Collections.Generic.List[string]]::new()
  $out = [IO.MemoryStream]::new()
  try {
    $proc = [Diagnostics.Process]::Start($psi)
    $outTask = $proc.StandardOutput.BaseStream.CopyToAsync($out)
    try { $proc.StandardInput.Close() } catch { }
    $task = $proc.StandardError.ReadLineAsync()
    while ($true) {
      if ($task.Wait(50)) {
        $line = $task.Result
        if ($null -eq $line) { break }
        $lines.Add($line)
        if (-not $released -and $line.Trim() -ceq $RetryLine) { $Hold.Dispose(); $released = $true }
        $task = $proc.StandardError.ReadLineAsync()
      } elseif ([DateTime]::UtcNow -gt $deadline) { $timedOut = $true; break }
    }
    $left = [Math]::Max(1000, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
    if (-not $timedOut -and -not $proc.WaitForExit($left)) { $timedOut = $true }
    if ($timedOut) { try { $proc.Kill($true) } catch { }; [void]$proc.WaitForExit(10000) }
    [void]$outTask.Wait(10000)
  } finally { $Hold.Dispose() }
  [pscustomobject]@{ ExitCode = $(if ($timedOut) { -1 } else { $proc.ExitCode }); Stdout = [Text.UTF8Encoding]::new($false).GetString($out.ToArray()); Stderr = ($lines -join "`n"); TimedOut = $timedOut; Released = $released }
}

$h = New-ConformanceHome
$h2 = $null
try {
  $name0 = "${sec}: the ProxyCommand refuses a lease alias whose lease is not ready"
  $program = Get-ConformanceProgram -Variable 'AGENTSPACE_SSH_PROXY_CMD'
  $gate = Test-AwsUnreachable -ConformanceHome $h
  if (-not $gate.Unreachable) {
    Write-ConformanceResult -Name $name0 -Status SKIPPED -Reason $gate.Reason
  } elseif (-not $program) {
    Write-ConformanceResult -Name $name0 -Status FAIL -Reason 'Neither AGENTSPACE_SSH_PROXY_CMD nor its default names a program'
  } else {
    $S = New-HpStandIns $h
    $null = New-HpPushFixture $h
    $leasesPath = Get-HpLeasesPath $h
    $leasesLock = Join-Path $h.StateDir 'leases.lock'
    $registry = @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-HpHostRecord 'other' 'ready' $HpIdOther), (New-HpHostRecord 'third' 'creating' $HpIdThird))
    Set-HpRegistry $h $registry
    $id = @{}
    foreach ($n in 1..13) { $id[$n] = New-NrId $n }
    $nowSecond = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
    $storedFive = 'agentspace-other-9f8e7d6c'
    $storedEleven = 'agentspace-other-9f8e7d6d'
    $alias10 = Get-NrAlias 'other' $id[10]
    $refusals = @(
      @{ N = 'that is released, with an expires_at'; State = 'released'; Alias = (Get-NrAlias 'other' $id[1]); Rec = (New-NrRecord $id[1] 'other' 'released' $future) },
      @{ N = 'that is released from pending, with no expires_at'; State = 'released'; Alias = (Get-NrAlias 'other' $id[2]); Rec = (New-NrRecord $id[2] 'other' 'released') },
      @{ N = 'that is stored ready past its expires_at'; State = 'expired'; Alias = (Get-NrAlias 'other' $id[3]); Rec = (New-NrRecord $id[3] 'other' 'ready' $past) },
      @{ N = 'that is stored ready with its expires_at the second it was seeded'; State = 'expired'; Alias = (Get-NrAlias 'other' $id[4]); Rec = (New-NrRecord $id[4] 'other' 'ready' $nowSecond) },
      @{ N = 'that is stored ready past its expires_at, found by its stored ssh.host_alias'; State = 'expired'; Alias = $storedFive; Rec = (New-NrRecord $id[5] 'other' 'ready' $past $storedFive) },
      @{ N = 'that is unsatisfiable with no_capacity'; State = 'unsatisfiable'; Alias = (Get-NrAlias 'other' $id[6]); Rec = (New-NrRecord $id[6] 'other' 'unsatisfiable') },
      @{ N = 'that is unsatisfiable with plan_too_large'; State = 'unsatisfiable'; Alias = (Get-NrAlias 'other' $id[7]); Rec = (New-NrRecord $id[7] 'other' 'unsatisfiable' '' '' 'plan_too_large') },
      @{ N = 'that is pending'; State = 'pending'; Alias = (Get-NrAlias 'other' $id[8]); Rec = (New-NrRecord $id[8] 'other' 'pending') },
      @{ N = 'that is released, of a project with no host record'; State = 'released'; Alias = (Get-NrAlias 'ghost' $id[9]); Rec = (New-NrRecord $id[9] 'ghost' 'released') }
    )
    $controls = @(
      @{ N = 'a ready lease, found by the alias built from its record, reaches its ready host'; A = $alias10; Want = $HpIdOther },
      @{ N = 'a ready lease given the alias built from its record, whose stored ssh.host_alias is another, reaches its ready host'; A = (Get-NrAlias 'other' $id[11]); Want = $HpIdOther },
      @{ N = 'a ready lease given its stored ssh.host_alias, which is not the built alias, reaches its ready host'; A = $storedEleven; Want = $HpIdOther },
      @{ N = 'a ready lease of a host that is not ready reaches the box, and is not refused'; A = (Get-NrAlias 'third' $id[12]); Want = $HpIdBox },
      @{ N = 'a ready lease of a project with no host record reaches the box, and is not refused'; A = (Get-NrAlias 'ghost' $id[13]); Want = $HpIdBox },
      @{ N = 'a lease alias that is no record''s alias reaches the box'; A = 'agentspace-nope-12345678'; Want = $HpIdBox },
      @{ N = 'the argument agentspace reaches the box'; A = 'agentspace'; Want = $HpIdBox }
    )
    $all = @($refusals | ForEach-Object { $_.Rec }) + @(
      (New-NrRecord $id[10] 'other' 'ready' $future),
      (New-NrRecord $id[11] 'other' 'ready' $future $storedEleven),
      (New-NrRecord $id[12] 'third' 'ready' $future),
      (New-NrRecord $id[13] 'ghost' 'ready' $future))
    Set-NrLeases $h $all
    $leasesHash = Get-HpBytesHash $leasesPath
    $before = Get-NrSnapshot $h

    # ---- Each state refused, with and without AGENTSPACE_DIAGNOSTIC ----
    foreach ($c in $refusals) {
      foreach ($diag in $false, $true) {
        $extra = if ($diag) { @{ AGENTSPACE_DIAGNOSTIC = '1' } } else { @{} }
        $r = Invoke-NrRefused $h $S $c.Alias $extra
        $what = if ($diag) { ', and AGENTSPACE_DIAGNOSTIC adds no line' } else { '' }
        Write-HpProblems "${sec}: The alias of a lease $($c.N) is refused as $($c.State): exit 1, one line, no tunnel, no AWS call, no ssh or scp, no file changed$what" (Get-NrRefusalProblems $r $S $h $before $c.State $c.Alias)
      }
    }

    # ---- The refusal comes before the host registry is read ----
    foreach ($c in $refusals[0], $refusals[2], $refusals[8]) {
      Remove-HpPath $h (Get-HpRegistryPath $h)
      Set-HpFile $h (Get-HpRegistryPath $h) "this is not json`n"
      $snap = Get-NrSnapshot $h
      $r = Invoke-NrRefused $h $S $c.Alias
      Write-HpProblems "${sec}: The alias of a lease $($c.N) is refused as $($c.State) before the host registry is read: a registry that is not valid is neither read nor repaired" (Get-NrRefusalProblems $r $S $h $snap $c.State $c.Alias)
    }
    Set-HpRegistry $h $registry

    # ---- A ready lease still connects; what is no refusal reaches the box ----
    foreach ($c in $controls) {
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @($c.A)
      Write-HpProblems "${fromSec}: $($c.N)" (Get-HpReachedProblems $r $S $c.Want)
    }
    $p = New-HpProblems
    if ((Get-HpBytesHash $leasesPath) -cne $leasesHash) { $p.Add('The lease state file changed') }
    if (Test-Path -LiteralPath $leasesLock) { $p.Add('The lease state lock file was made, so the lock was taken') }
    Write-HpProblems "${fromSec}: The ProxyCommand never writes the lease state file or takes its lock, refusing or connecting" $p
    Remove-HpPath $h $leasesPath
    Clear-HpLogs $h $S
    $r = Invoke-HpProxy $h $S @($alias10)
    Write-HpProblems "${fromSec}: No lease state file reaches the box for a lease alias, and refuses nothing" (Get-HpReachedProblems $r $S $HpIdBox)

    # ---- A lease state file that cannot be read or is not valid (pin P21) ----
    $rec10 = New-NrRecord $id[10] 'other' 'ready' $future
    $formless = @('agentspace', ('agentspace-other-' + $id[10].Substring(0, 8).ToUpperInvariant()), ('agentspace-other-' + $id[10].Substring(0, 7)), ('agentspace-other-' + $id[10].Substring(0, 8) + '1'))
    $unreadable = @(
      @{ N = 'is not JSON'; Text = 'this is not json'; Every = $true },
      @{ N = 'has version 1'; Text = '{"version":1,"leases":[' + $rec10 + ']}' },
      @{ N = 'holds two records with one id'; Text = '{"version":0,"leases":[' + $rec10 + ',' + $rec10 + ']}' },
      @{ N = 'holds a record that does not fit its schema'; Text = '{"version":0,"leases":[' + $rec10.Replace('"state":"ready"', '"state":"weird"') + ']}' },
      @{ N = 'is a folder at its path'; Folder = $true; Every = $true }
    )
    foreach ($c in $unreadable) {
      Remove-HpPath $h $leasesPath
      if ($c.Folder) { New-Item -ItemType Directory -Path (Assert-ConformancePath -ConformanceHome $h -Path $leasesPath) | Out-Null } else { Set-HpFile $h $leasesPath ($c.Text + "`n") }
      $snap = Get-NrSnapshot $h
      $r = Invoke-NrRefused $h $S $alias10
      Write-HpProblems "${sec}: A lease state file that $($c.N) refuses a lease alias, whatever it names: exit 1, one line saying the lease records could not be read and naming no file, no tunnel, no AWS call, no file changed (P21)" (Get-NrRefusalProblems $r $S $h $snap '' $alias10)
      $r = Invoke-NrRefused $h $S $alias10 @{ AGENTSPACE_DIAGNOSTIC = '1' }
      $p = Get-NrRefusalProblems $r $S $h $snap '' $alias10 -FileLine
      if ((Get-NrRetryCount $r) -ne 0) { $p.Add("There are $(Get-NrRetryCount $r) retry lines, not 0") }
      Write-HpProblems "${sec}: A lease state file that $($c.N), with AGENTSPACE_DIAGNOSTIC set: the line naming the file, then the refusal as the last line, and no retry line (P21, P23, P27)" $p
      $forms = if ($c.Every) { $formless } else { @($formless[0]) }
      foreach ($a in $forms) {
        Clear-HpLogs $h $S
        $r = Invoke-HpProxy $h $S @($a)
        Write-HpProblems "${fromSec}: A lease state file that $($c.N) still sends the argument $a, which is not of lease alias form, to the box" (Get-HpReachedProblems $r $S $HpIdBox)
      }
      Clear-HpLogs $h $S
      $r = Invoke-HpProxy $h $S @('agentspace') -Extra @{ AGENTSPACE_DIAGNOSTIC = '1' }
      $p = Get-HpReachedProblems $r $S $HpIdBox
      if ($r.Stderr -notmatch 'leases\.json') { $p.Add('With AGENTSPACE_DIAGNOSTIC set, no line names the lease state file') }
      Write-HpProblems "${fromSec}: A lease state file that $($c.N), with AGENTSPACE_DIAGNOSTIC set, is named on stderr, and the argument agentspace still reaches the box" $p
    }

    # ---- The sharing-violation retry (pins P23 and P27) ----
    Set-NrLeases $h @($rec10)
    $snap = Get-NrSnapshot $h
    $leasesHash = Get-HpBytesHash $leasesPath
    $fs = [IO.File]::Open($leasesPath, 'Open', 'Read', 'None')
    try {
      $r = Invoke-NrRefused $h $S $alias10 @{} 10
      $p1 = Get-NrRefusalProblems $r $S $h $null '' $alias10
      $r = Invoke-NrRefused $h $S $alias10 @{ AGENTSPACE_DIAGNOSTIC = '1' } 10
      $p2 = Get-NrRefusalProblems $r $S $h $null '' $alias10 -FileLine
      if ((Get-NrRetryCount $r) -ne 2) { $p2.Add("There are $(Get-NrRetryCount $r) retry lines, not 2") }
    } finally { $fs.Dispose() }
    Write-HpProblems "${sec}: A lease state file the case holds open without sharing past the read's bound refuses a lease alias: exit 1, one line, no tunnel, within 10 seconds (P23)" $p1
    Write-HpProblems "${sec}: The same hold, with AGENTSPACE_DIAGNOSTIC set: two retry lines, the line naming the file, then the refusal as the last line (P23, P27)" $p2
    Write-HpProblems "${sec}: The hold changed nothing in the home" (Get-NrSnapshotProblems $snap $h)

    # The same hold past the read's bound still sends an argument that is not of lease alias form to the box, with
    # AGENTSPACE_DIAGNOSTIC set: the two retry lines, then the line naming the file (operator.md, "Which checks show
    # it", v0.11's row for a lease state file that cannot be read; P21, P23, P27).
    foreach ($a in $formless[0..2]) {
      Clear-HpLogs $h $S
      $fs = [IO.File]::Open($leasesPath, 'Open', 'Read', 'None')
      try { $r = Invoke-HpProxy $h $S @($a) -Extra @{ AGENTSPACE_DIAGNOSTIC = '1' } -Timeout 10 } finally { $fs.Dispose() }
      $p = Get-HpReachedProblems $r $S $HpIdBox
      $lines = @($r.Stderr -split "`r?`n" | Where-Object { $_.Trim() -ne '' } | ForEach-Object { $_.Trim() })
      $retryAt = @(for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i] -ceq $RetryLine) { $i } })
      $fileAt = @(for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i] -match 'leases\.json') { $i } })
      if ($retryAt.Count -ne 2) { $p.Add("There are $($retryAt.Count) retry lines, not 2") }
      if ($fileAt.Count -ne 1) { $p.Add('There is not exactly one line naming the lease state file') }
      elseif ($retryAt.Count -gt 0 -and $retryAt[-1] -gt $fileAt[0]) { $p.Add('A retry line comes after the line naming the file') }
      Write-HpProblems "${fromSec}: The same hold still sends the argument $a, which is not of lease alias form, to the box, with AGENTSPACE_DIAGNOSTIC set: two retry lines, then the line naming the file (P21, P23, P27)" $p
    }

    Clear-HpLogs $h $S
    $fs = [IO.File]::Open($leasesPath, 'Open', 'Read', 'None')
    $r = Invoke-NrHeld $h $S $alias10 $fs
    $p = Get-HpReachedProblems $r $S $HpIdOther
    $n = Get-NrRetryCount $r
    if ($n -lt 1 -or $n -gt 2) { $p.Add("There are $n retry lines, not 1 or 2") }
    if (-not $r.Released) { $p.Add('The hold was never released on a retry line') }
    Write-HpProblems "${sec}: A hold the case releases on the first retry line to reach the ProxyCommand's stderr lets the read through: the ready lease's alias connects, after one or two retry lines, within 10 seconds (P23, P27)" $p
    $p = New-HpProblems
    if ((Get-HpBytesHash $leasesPath) -cne $leasesHash) { $p.Add('The lease state file changed') }
    if (Test-Path -LiteralPath $leasesLock) { $p.Add('The lease state lock file was made, so the lock was taken') }
    Write-HpProblems "${fromSec}: A read that was held and let through neither writes the lease state file nor takes its lock" $p

    # ---- Two records that are one alias's (pin P22) ----
    $idA = 'c0000001-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
    $idB = 'c0000001-bbbb-4bbb-8bbb-bbbbbbbbbbbb'
    $aliasC = Get-NrAlias 'other' $idA
    $pairs = @(
      @{ Recs = @('released', 'ready'); Want = '' }, @{ Recs = @('ready', 'released'); Want = '' },
      @{ Recs = @('expired', 'ready'); Want = '' }, @{ Recs = @('ready', 'expired'); Want = '' },
      @{ Recs = @('released', 'pending'); Want = 'pending' }, @{ Recs = @('pending', 'released'); Want = 'released' },
      @{ Recs = @('released', 'expired'); Want = 'expired' }, @{ Recs = @('expired', 'released'); Want = 'released' }
    )
    foreach ($c in $pairs) {
      $first = $c.Recs[0]
      $second = $c.Recs[1]
      Set-NrLeases $h @((New-NrSpecRecord $first $idA), (New-NrSpecRecord $second $idB))
      $order = "records $first then $second with one alias"
      if ($c.Want) {
        $snap = Get-NrSnapshot $h
        $r = Invoke-NrRefused $h $S $aliasC
        Write-HpProblems "${fromSec}: Two $order are refused as $($c.Want), the later one's state, and no connection is made (P22)" (Get-NrRefusalProblems $r $S $h $snap $c.Want $aliasC)
      } else {
        Clear-HpLogs $h $S
        $r = Invoke-HpProxy $h $S @($aliasC)
        Write-HpProblems "${fromSec}: Two $order connect through the first one that is ready, an expired one being skipped (P22)" (Get-HpReachedProblems $r $S $HpIdOther)
      }
    }

    # ---- A release followed by a ProxyCommand started for its alias ----
    $name2 = "${sec}: A ready lease released by the build's lease release is refused as released, with no AWS call"
    $h2 = New-ConformanceHome
    if (-not $h2.CaReady) {
      Write-ConformanceResult -Name $name2 -Status SKIPPED -Reason $h2.CaReason
    } else {
      $S2 = New-HpStandIns $h2
      Set-HpRegistry $h2 @((New-HpHostRecord 'swimcoach' 'ready' $HpIdBox), (New-HpHostRecord 'conf' 'ready' $HpIdOther))
      $cliEnv = @{ AGENTSPACE_AWS_CMD = $S2.Aws }
      $c = Invoke-AgentspaceCli -ConformanceHome $h2 -Arguments @('lease', 'create', '--project', 'conf') -Environment $cliEnv
      $doc = ConvertFrom-ConformanceJson $c.Stdout
      $okDoc = $c.ExitCode -eq 0 -and $doc -and $doc['state'] -ceq 'ready' -and $doc['id'] -cmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' -and $doc['ssh']['host_alias'] -cmatch '^agentspace-conf-[0-9a-f]{8}$'
      if (-not $okDoc) {
        Write-ConformanceResult -Name $name2 -Status FAIL -Reason 'The build''s create did not answer a ready lease with its ID and alias'
      } else {
        $leaseId = [string]$doc['id']
        $leaseAlias = [string]$doc['ssh']['host_alias']
        Clear-HpLogs $h2 $S2
        $r = Invoke-HpProxy $h2 $S2 @($leaseAlias)
        Write-HpProblems "${fromSec}: The lease the build made, before its release, connects to its ready host (the control)" (Get-HpReachedProblems $r $S2 $HpIdOther)
        $rel = Invoke-AgentspaceCli -ConformanceHome $h2 -Arguments @('lease', 'release', $leaseId) -Environment $cliEnv
        if ($rel.ExitCode -ne 0) {
          Write-ConformanceResult -Name $name2 -Status FAIL -Reason "The build's release exited $($rel.ExitCode), not 0"
        } else {
          $snap2 = Get-NrSnapshot $h2
          Clear-HpLogs $h2 $S2
          $r = Invoke-HpProxy $h2 $S2 @($leaseAlias)
          Write-HpProblems $name2 (Get-NrRefusalProblems $r $S2 $h2 $snap2 'released' $leaseAlias)
          Clear-HpLogs $h2 $S2
          $r = Invoke-HpOperator $h2 $S2 @('up', '-Host', 'conf', '-Alias', $leaseAlias) @{ AGENTSPACE_NESTED = '1' }
          $p = New-HpProblems
          if ($r.TimedOut) { $p.Add('It timed out') }
          elseif ($r.ExitCode -ne 0) { $p.Add("Exit $($r.ExitCode), not 0") }
          if ($r.Stderr -match 'no connection was opened') { $p.Add('A person''s up refused the alias') }
          Write-HpProblems "${fromSec}: A person's up -Alias for the released lease's alias is not refused (up never reads the lease state file)" $p
        }
      }
    }
  }
} catch {
  $why = ((ConvertTo-ConformanceMasked $_.Exception.Message) -replace '[A-Za-z]:\\[^\s''"]*', '<path>' -replace '\s+', ' ')
  Write-ConformanceResult -Name 'test-unmet-proxy-not-ready.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$why at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h2) { Remove-ConformanceHome -ConformanceHome $h2 }
  Remove-ConformanceHome -ConformanceHome $h
}
Exit-ConformanceRun

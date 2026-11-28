# Step 220: lease B, create --size s while lease A is ready: ready at once (D66, a larger host with another live ready
# lease), then status again: Type still m8a.xlarge (no resize). Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. B's ID goes to lease-B.id, never printed. B carries --ttl too, so it
# cannot outlive the check by 8 hours (the task's line for 220 names no TTL: a question for the dev lead).
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '220' "lease B: create --size s while lease A is ready, then the host's type again"
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $a = Get-S01LeaseId 'A'
    if (-not $a) { Write-S01 'FINDING: The state file holds no lease A; steps 210 to 212 have not run.'; return 1 }
    if (Get-S01State 'lease-B.id') { Write-S01 'FINDING: A lease B is already recorded in the state file, so this step ran before; it makes no second one.'; return 1 }
    $h = Test-S01Hours 'Before lease B'
    if ($null -ne $h) { return $h }
    Write-S01 "Command: agentspace lease create --platform $($script:S01Platform) --project $($script:S01Project) --size s --ttl $($script:S01Ttl)"
    $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'create', '--platform', $script:S01Platform, '--project', $script:S01Project, '--size', 's', '--ttl', $script:S01Ttl) -TimeoutSec 180
    if (-not $run.StartFailed -and $run.StdoutBytes.Length -gt 0) {
      [IO.File]::WriteAllBytes((Get-S01StatePath 'doc-220.json'), $run.StdoutBytes)
      $d = ConvertFrom-S01Json $run.Stdout.Trim()
      if ($d -is [hashtable] -and (Test-S01Uuid $d['id'])) { Set-S01State 'lease-B.id' $d['id']; Write-S01 'Lease B: its ID is in the state file (never printed).' }
    }
    $want = @{ State = 'ready'; Size = 's'; Members = @('id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'expires_at', 'ssh'); User = 'ubuntu'; Ttl = $script:S01TtlSeconds; Expires = 'future'; NotId = $a; Alias = 'match' }
    $doc = Test-S01Lease -Run $run -Want $want -Problems $p -StepName '220'
    if ($null -ne $doc -and $doc['state'] -ceq 'pending') {
      Write-S01 'The lease is pending: D66''s second row (a ready host larger than asked, another live ready lease) was not met, and a resizing run MAY be under way (docs\decisions.md:3747-3748). Nothing is retried.'
    }
    # The alias differs from A's.
    $f = Get-S01StatePath 'doc-212.json'
    if ($null -ne $doc -and $doc['ssh'] -is [hashtable] -and (Test-Path -LiteralPath $f -PathType Leaf)) {
      $da = ConvertFrom-S01Json ($script:S01Utf8.GetString([IO.File]::ReadAllBytes($f)).Trim())
      if ($da -is [hashtable] -and $da['ssh'] -is [hashtable]) {
        $differs = ($da['ssh']['host_alias'] -cne $doc['ssh']['host_alias'])
        Write-S01 "Lease B's alias differs from lease A's: $differs"
        if (-not $differs) { $p.Add('lease B has the same alias as lease A') }
      }
    }
    # The host's type again: no resize.
    Write-S01 'The host again, after lease B:'
    Test-S01HostStatus -Problems $p
    $l = Read-S01LeaseList -Problems $p
    if ($null -ne $l) {
      Write-S01 "Leases of the project on the platform that are ready: $($l.OursReady.Count) (expected 2: A and B)"
      if ($l.OursReady.Count -ne 2) { $p.Add("$($l.OursReady.Count) lease(s) of the project on $($script:S01Platform) are ready, expected 2") }
    }
    return (Complete-S01 $p)
  })

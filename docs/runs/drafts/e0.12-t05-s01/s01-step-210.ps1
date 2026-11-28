# Step 210: lease A, create --platform linux/amd64 --project agentspace at the default size, TTL 2 hours (pin P3 pending).
# Run through scripts\live\wrap.ps1. RFC 2119 keywords; the capitals are load-bearing.
# The answer MUST be pending (docs\decisions.md:5091-5092). Its ID goes to the state file lease-A.id and is never printed.
# This step starts the host's making, so it first reads the observer's hours (exit 6 when they cannot be read or reach 1.75).
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '210' "lease A: create --platform $($script:S01Platform) --project $($script:S01Project) --ttl $($script:S01Ttl), the default size"
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $h = Test-S01Hours 'Before lease A'
    if ($null -ne $h) { return $h }
    if (Get-S01State 'lease-A.id') {
      Write-S01 'FINDING: A lease A is already recorded in the state file, so this step ran before; it makes no second one. Release the first (step 230) or remove the state folder by hand.'
      return 1
    }
    Write-S01 "Command: agentspace lease create --platform $($script:S01Platform) --project $($script:S01Project) --ttl $($script:S01Ttl)"
    $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'create', '--platform', $script:S01Platform, '--project', $script:S01Project, '--ttl', $script:S01Ttl) -TimeoutSec 180
    # The raw document and the ID, kept before anything else is checked: a lease made is a lease to release.
    if (-not $run.StartFailed -and $run.StdoutBytes.Length -gt 0) {
      [IO.File]::WriteAllBytes((Get-S01StatePath 'doc-210.json'), $run.StdoutBytes)
      $d = ConvertFrom-S01Json $run.Stdout.Trim()
      if ($d -is [hashtable] -and (Test-S01Uuid $d['id'])) { Set-S01State 'lease-A.id' $d['id']; Write-S01 'Lease A: its ID is in the state file (never printed).' }
    }
    $want = @{ State = 'pending'; Size = 'm'; Members = @('id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at'); User = $null; Ttl = $script:S01TtlSeconds; Expires = 'none'; Alias = 'none' }
    $doc = Test-S01Lease -Run $run -Want $want -Problems $p -StepName '210'
    if ($null -ne $doc -and $doc['state'] -ceq 'ready') { $p.Add('the lease is ready at once, so a host of the project already served it; the answer MUST be pending for the first lease on this platform') }
    return (Complete-S01 $p)
  })

# Step 212: lease get <A>: ready, ssh.user ubuntu, agent-linux, size m, an alias present. Run through scripts\live\wrap.ps1.
# RFC 2119 keywords; the capitals are load-bearing. The raw document is kept as doc-212.json for step 213 and never printed.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's01-common.ps1')
exit (Invoke-S01Main {
    $stop = Start-S01Step '212' 'lease A as get prints it: ready, ssh.user ubuntu, agent-linux, size m'
    if ($null -ne $stop) { return $stop }
    $p = New-S01List
    $id = Get-S01LeaseId 'A'
    if (-not $id) { Write-S01 'FINDING: The state file holds no lease A; step 210 has not run.'; return 1 }
    Write-S01 'Command: agentspace lease get <lease A>'
    $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'get', $id) -TimeoutSec 120
    if (-not $run.StartFailed -and $run.StdoutBytes.Length -gt 0) { [IO.File]::WriteAllBytes((Get-S01StatePath 'doc-212.json'), $run.StdoutBytes) }
    $want = @{ State = 'ready'; Size = 'm'; Members = @('id', 'project', 'state', 'platform', 'profile', 'size', 'ttl_seconds', 'created_at', 'expires_at', 'ssh'); User = 'ubuntu'; Ttl = $script:S01TtlSeconds; Expires = 'future'; Id = $id; Alias = 'match' }
    [void](Test-S01Lease -Run $run -Want $want -Problems $p -StepName '212')
    return (Complete-S01 $p)
  })

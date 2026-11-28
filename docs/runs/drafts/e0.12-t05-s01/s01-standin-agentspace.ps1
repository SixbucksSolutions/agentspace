# The dry runs' stand-in for agentspace.exe (lease create, get, release, list), answering s01-expected.md.
# RFC 2119 keywords; the capitals are load-bearing. See s01-standin-lib.ps1. Run as `pwsh -File <this> <the CLI's arguments>`.
# A pending lease becomes ready at its S01_STANDIN_READY_AFTER-th get (default 3).
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 's01-standin-lib.ps1')
$plant = Get-SlPlant
$argv = @($args | ForEach-Object { [string]$_ })
if ($argv.Count -lt 2 -or $argv[0] -cne 'lease') { Write-SlError 'usage' 'Only lease commands are stood in.'; exit 2 }
$readyAfter = if ($env:S01_STANDIN_READY_AFTER) { [int]$env:S01_STANDIN_READY_AFTER } else { 3 }
$store = Read-SlStore

function Find-SlLease([string]$Id) { foreach ($l in $store) { if ($l['id'] -ceq $Id) { return $l } }; return $null }
function Set-SlAt($Old, $New) { for ($i = 0; $i -lt $store.Count; $i++) { if ($store[$i]['id'] -ceq $Old['id']) { $store[$i] = $New } } }

switch ($argv[1]) {
  'create' {
    $flag = @{}
    for ($i = 2; $i -lt $argv.Count; $i += 2) { $flag[$argv[$i]] = $argv[$i + 1] }
    $size = if ($flag.ContainsKey('--size')) { $flag['--size'] } else { 'm' }
    $ttl = switch ($flag['--ttl']) { '2h' { 7200 } '8h' { 28800 } default { 28800 } }
    $anyReady = @($store | Where-Object { $_['state'] -ceq 'ready' -and $_['project'] -ceq $flag['--project'] }).Count -gt 0
    $state = if ($anyReady) { 'ready' } else { 'pending' }
    $l = New-SlLease -State $state -Size $size -Ttl $ttl -Project $flag['--project'] -Platform $flag['--platform']
    $isB = ($size -ceq 's')
    switch ($plant) {
      'create-ready' { $l = Set-SlReady $l }
      'create-platform' { $l['platform'] = 'linux/arm64' }
      'create-size' { $l['size'] = 'l' }
      'create-ttl' { $l['ttl_seconds'] = 28800 }
      'create-profile' { $l['profile'] = 'agent-windows' }
      'create-member' { $l['sized_for'] = [ordered]@{ turn = 'T01'; agents = 3 } }
      'create-project' { $l['project'] = 'swimcoach' }
      'create-bad-id' { $l['id'] = 'not-a-uuid' }
      'create-unsat' { $l['state'] = 'unsatisfiable'; $l['reasons'] = @([ordered]@{ code = 'no_capacity'; message = "No capacity can be found for project 'agentspace' on platform 'linux/amd64': it has no host there and is not approved for that platform on agentspace's project list." }) }
      'b-pending' { if ($isB) { $l = New-SlLease -State 'pending' -Size $size -Ttl $ttl -Project $flag['--project'] -Platform $flag['--platform'] } }
      'b-size' { if ($isB) { $l['size'] = 'm' } }
      'b-user' { if ($isB) { $l['ssh']['user'] = 'root' } }
      'b-same-alias' { if ($isB) { $first = @($store | Where-Object { $_['state'] -ceq 'ready' })[0]; $l['ssh']['host_alias'] = $first['ssh']['host_alias'] } }
    }
    if ($plant -ceq 'create-exit') { Write-SlError 'internal' "agentspace lease create failed: localswim's service receipt was not found. Nothing was recorded."; exit 1 }
    if ($plant -ceq 'create-stderr') { [Console]::Error.Write("warning: something\n") }
    if ($plant -cne 'create-bad-id') { $store.Add($l); Write-SlStore $store }
    Write-SlDoc (Get-SlPublic $l) -Pretty:($plant -ceq 'create-pretty')
    exit 0
  }
  'get' {
    $l = Find-SlLease $argv[2]
    if ($null -eq $l) { Write-SlError 'not_found' 'No lease has that ID.'; exit 3 }
    if ($plant -ceq 'get-exit') { Write-SlError 'internal' 'The lease records could not be read.'; exit 1 }
    if ($l['state'] -ceq 'pending') {
      $l['_gets'] = [int]$l['_gets'] + 1
      if ($l['_gets'] -ge $readyAfter -and $plant -cne 'get-pending') { $l = Set-SlReady $l }
      Set-SlAt $l $l
      for ($i = 0; $i -lt $store.Count; $i++) { if ($store[$i]['id'] -ceq $argv[2]) { $store[$i] = $l } }
      Write-SlStore $store
    }
    $out = Get-SlPublic $l
    switch ($plant) {
      'get-unsat' { $out['state'] = 'unsatisfiable'; $out.Remove('expires_at'); $out.Remove('ssh'); $out['reasons'] = @([ordered]@{ code = 'no_capacity'; message = 'No capacity.' }) }
      'get-expired' { $out['state'] = 'expired'; $out.Remove('ssh') }
      'get-user' { if ($out.Contains('ssh')) { $out['ssh']['user'] = 'root' } }
      'get-noalias' { if ($out.Contains('ssh')) { $out['ssh'].Remove('host_alias') } }
      'get-alias-form' { if ($out.Contains('ssh')) { $out['ssh']['host_alias'] = ('agentspace-agentspace-' + 'dead' + 'beef') } }
      'get-size' { $out['size'] = 'l' }
      'get-profile' { $out['profile'] = 'agent-windows' }
      'get-platform' { $out['platform'] = 'linux/arm64' }
      'get-expires-past' { if ($out.Contains('expires_at')) { $out['expires_at'] = Format-SlTime ([DateTimeOffset]::UtcNow.AddMinutes(-5)) } }
      'get-expires-far' { if ($out.Contains('expires_at')) { $out['expires_at'] = Format-SlTime ([DateTimeOffset]::UtcNow.AddHours(8)) } }
      'get-no-ssh' { $out.Remove('ssh') }
      'get-state-pending' { $out['state'] = 'pending'; $out.Remove('expires_at'); $out.Remove('ssh') }
      'get-ttl' { $out['ttl_seconds'] = 3600 }
    }
    Write-SlDoc $out
    exit 0
  }
  'release' {
    $l = Find-SlLease $argv[2]
    if ($null -eq $l) { Write-SlError 'not_found' 'No lease has that ID.'; exit 3 }
    if ($plant -ceq 'release-exit-b' -and $l['size'] -ceq 's') { Write-SlError 'internal' 'The lease records could not be written.'; exit 1 }
    $out = Get-SlPublic $l
    if ($l['state'] -ceq 'ready' -and $plant -cne 'release-ready') { $out['state'] = 'released'; $out.Remove('ssh') }
    if ($plant -cne 'release-no-time') { $out['released_at'] = Format-SlTime ([DateTimeOffset]::UtcNow) }
    if ($plant -ceq 'release-ssh') { $out['ssh'] = $l['ssh'] }
    $keep = [ordered]@{}
    foreach ($k in $out.Keys) { $keep[$k] = $out[$k] }
    $new = [ordered]@{}
    foreach ($k in $keep.Keys) { $new[$k] = $keep[$k] }
    $new['_gets'] = $l['_gets']
    if ($plant -cne 'release-ready') { for ($i = 0; $i -lt $store.Count; $i++) { if ($store[$i]['id'] -ceq $argv[2]) { $store[$i] = $new } }; Write-SlStore $store }
    Write-SlDoc $out
    exit 0
  }
  'list' {
    $docs = @($store | ForEach-Object { Get-SlPublic $_ })
    if ($plant -ceq 'list-ours-ready') { $x = New-SlLease -State 'ready' -Size 'm'; $docs += (Get-SlPublic $x) }
    if ($plant -ceq 'list-other-ready') { $x = New-SlLease -State 'ready' -Size 'm' -Project 'swimcoach' -Platform 'linux/arm64'; $docs += (Get-SlPublic $x) }
    if ($plant -ceq 'list-invalid') { [Console]::Out.Write('{"leases":"x"}' + "`n"); exit 0 }
    Write-SlDoc ([ordered]@{ leases = @($docs) })
    exit 0
  }
  default { Write-SlError 'usage' 'No such lease command.'; exit 2 }
}

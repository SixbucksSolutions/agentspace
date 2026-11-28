# Step 312 of E0.12-T05-S02: `lease get` on the ready lease W: state ready, ssh.user agent, profile agent-windows, and
# whether ssh.host_alias is present (True/False; the alias itself is never printed). Run through scripts\live\wrap.ps1 (-Script).
# RFC 2119 keywords; the capitals are load-bearing. The alias goes to the state folder (lease-w.alias) for steps 313 on.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '312' 'lease get: ready, ssh.user agent, profile agent-windows, host_alias present')) { exit 5 }
if (-not (Test-S02State 'lease-w.id')) { Stop-S02Step 'no lease W state file; step 310 has not run.'; exit 5 }
exit (Invoke-S02Main {
    $id = Read-S02State 'lease-w.id'
    $r = Invoke-S02Tool -Tool 'agentspace' -Arguments @('lease', 'get', $id) -TimeoutSeconds 120
    Write-S02Line "lease get exit: $($r.ExitCode)"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'lease get exited 0.' "lease get exited $($r.ExitCode)."
    $text = $r.Stdout
    $doc = ConvertFrom-S02Json $text.TrimEnd("`n")
    if ($null -eq $doc -or $doc -isnot [System.Collections.IDictionary]) { Add-S02Finding 'lease get did not print a JSON object.'; return }
    Write-S02State 'lease-w.json' $text
    $ssh = if ($doc.ContainsKey('ssh') -and $doc['ssh'] -is [System.Collections.IDictionary]) { $doc['ssh'] } else { $null }
    $members = (Get-S02SortedKeys $doc) -join ','
    Write-S02Line "Members: $members"
    Write-S02Line "State: '$($doc['state'])'; platform: '$($doc['platform'])'; profile: '$($doc['profile'])'; size: '$($doc['size'])'; ttl_seconds: '$($doc['ttl_seconds'])'"
    if ($doc['expires_at'] -is [string]) { Write-S02Line ('expires_at: ' + (Format-S02Time $doc['expires_at'])) }
    Assert-S02 ($doc['state'] -ceq 'ready') 'The lease is ready.' "The lease's state is '$($doc['state'])', expected 'ready'."
    Assert-S02 ($doc['platform'] -ceq 'windows/amd64') 'The platform is windows/amd64.' "The platform is '$($doc['platform'])'."
    Assert-S02 ($doc['profile'] -ceq 'agent-windows') 'The profile is agent-windows.' "The profile is '$($doc['profile'])', expected 'agent-windows'."
    Assert-S02 ($members -ceq 'created_at,expires_at,id,platform,profile,project,size,ssh,state,ttl_seconds') 'The ready lease has expires_at and ssh and no reasons.' "The members are '$members'."
    $present = ($null -ne $ssh -and $ssh.ContainsKey('host_alias') -and $ssh['host_alias'] -is [string] -and $ssh['host_alias'] -ne '')
    Write-S02Line "ssh.host_alias present: $present"
    Assert-S02 $present 'ssh.host_alias is present.' 'ssh.host_alias is missing.'
    $user = if ($null -ne $ssh -and $ssh.ContainsKey('user')) { [string]$ssh['user'] } else { '' }
    Write-S02Line "ssh.user: '$user'"
    Assert-S02 ($user -ceq 'agent') 'ssh.user is agent.' "ssh.user is '$user', expected 'agent'."
    if ($present) {
      Write-S02State 'lease-w.alias' ([string]$ssh['host_alias'] + "`n")
      Assert-S02 ($ssh['host_alias'] -cmatch '^agentspace-agentspace-[0-9a-f]{8}$') 'The alias has the form agentspace-<project>-<8 hex digits>.' 'The alias does not have the form agentspace-agentspace-<8 hex digits>.'
    }
    $valid = Test-S02Schema $text.TrimEnd("`n") 'lease.output.schema.json'
    Assert-S02 $valid 'The document validates against lease.output.schema.json.' 'The document does not validate against lease.output.schema.json.'
  })

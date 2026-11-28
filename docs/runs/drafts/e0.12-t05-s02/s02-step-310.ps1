# Step 310 of E0.12-T05-S02: Lease W, `agentspace lease create --platform windows/amd64 --project agentspace --ttl 4h`, at the
# default size. Run through scripts\live\wrap.ps1 (-Script). RFC 2119 keywords; the capitals are load-bearing.
#
# - The TTL is S02_TTL, default 4h (pin P3, pending; lease-api.md, TTL: "8h", "1h30m"). The default size is `m`.
# - It refuses (STOP) when a state file of a lease W already exists: one lease W per check, since each costs instance hours.
#   Move the file aside by hand only after the lease it names is released.
# - The raw document and the ID go to the state folder (lease-w.json, lease-w.id) and are never printed. The log's line
#   count before the create goes to provlog-baseline.txt, for step 311.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 's02-common.ps1')
if (-not (Start-S02Step '310' 'Lease W: create --platform windows/amd64 --project agentspace, default size')) { exit 5 }
if (Test-S02State 'lease-w.json') { Stop-S02Step 'a state file of a lease W exists already; one lease W per check.'; exit 5 }
exit (Invoke-S02Main {
    $ttl = if ($env:S02_TTL) { $env:S02_TTL } else { '4h' }
    $wantTtl = $null
    if ($ttl -cmatch '^(\d+)h$') { $wantTtl = [int]$Matches[1] * 3600 }
    $baseline = @(Read-S02LogLines (Get-S02ProvisionLogPath)).Count
    Write-S02State 'provlog-baseline.txt' ([string]$baseline + "`n")
    Write-S02Line "provision.log lines before the create: $baseline"
    $args2 = @('lease', 'create', '--platform', 'windows/amd64', '--project', 'agentspace', '--ttl', $ttl)
    Write-S02Line ('Command: agentspace ' + ($args2 -join ' '))
    $r = Invoke-S02Tool -Tool 'agentspace' -Arguments $args2 -TimeoutSeconds 180
    Write-S02Line "Exit: $($r.ExitCode); elapsed $([int]$r.ElapsedSeconds) s"
    Assert-S02 ($r.ExitCode -eq 0 -and -not $r.TimedOut) 'create exited 0.' "create exited $($r.ExitCode)."
    Assert-S02 ($r.Stderr.Length -eq 0) 'create wrote nothing to stderr.' "create wrote $($r.Stderr.Length) characters to stderr."
    $text = $r.Stdout
    Write-S02State 'lease-w.json' $text
    $doc = ConvertFrom-S02Json $text.TrimEnd("`n")
    if ($null -eq $doc -or $doc -isnot [System.Collections.IDictionary]) {
      Add-S02Finding 'create did not print a JSON object.'
      return
    }
    if ($doc['id'] -is [string]) { Write-S02State 'lease-w.id' ([string]$doc['id'] + "`n") }
    Assert-S02 ($text.EndsWith("`n") -and $text.IndexOf("`n") -eq $text.Length - 1) 'The document is one line ending in one line feed.' 'The document is not one line ending in one line feed.'
    $members = (Get-S02SortedKeys $doc) -join ','
    Write-S02Line "Members: $members"
    Assert-S02 ($members -ceq 'created_at,id,platform,profile,project,size,state,ttl_seconds') 'The pending lease has the members of a lease with no connection yet.' "The members are '$members', expected 'created_at,id,platform,profile,project,size,state,ttl_seconds'."
    Write-S02Line "State: '$($doc['state'])'; platform: '$($doc['platform'])'; profile: '$($doc['profile'])'; project: '$($doc['project'])'; size: '$($doc['size'])'; ttl_seconds: '$($doc['ttl_seconds'])'; id: <lease>"
    Assert-S02 ($doc['state'] -ceq 'pending') 'The lease is pending.' "The lease's state is '$($doc['state'])', expected 'pending'."
    Assert-S02 ($doc['platform'] -ceq 'windows/amd64') 'The platform is windows/amd64.' "The platform is '$($doc['platform'])', expected 'windows/amd64'."
    Assert-S02 ($doc['profile'] -ceq 'agent-windows') 'The profile is agent-windows (the platform''s default).' "The profile is '$($doc['profile'])', expected 'agent-windows'."
    Assert-S02 ($doc['project'] -ceq 'agentspace') 'The project is agentspace.' "The project is '$($doc['project'])', expected 'agentspace'."
    Assert-S02 ($doc['size'] -ceq 'm') 'The size is the default, m.' "The size is '$($doc['size'])', expected the default 'm'."
    if ($null -ne $wantTtl) { Assert-S02 (($doc['ttl_seconds'] -is [int] -or $doc['ttl_seconds'] -is [long]) -and [long]$doc['ttl_seconds'] -eq $wantTtl) "ttl_seconds is $wantTtl." "ttl_seconds is '$($doc['ttl_seconds'])', expected $wantTtl." }
    Assert-S02 ($doc['id'] -is [string] -and $doc['id'] -cmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$') 'The id is a lowercase UUIDv4.' 'The id is not a lowercase UUIDv4.'
    $valid = Test-S02Schema $text.TrimEnd("`n") 'lease.output.schema.json'
    Assert-S02 $valid 'The document validates against lease.output.schema.json.' 'The document does not validate against lease.output.schema.json.'
  })

#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for story E1.05: agentspace lease renew, release and list (scripts\lease-cmd-*.ps1).
# - It runs the real CLI as a child process, `pwsh -NoProfile -File scripts\lease-cli.ps1 lease ...`,
#   with AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR set to throwaway folders under $env:TEMP.
# - It seeds the state file itself and MUST NOT call create. A ready lease gets a real credential and
#   alias from Invoke-LeaseDriverPlace, into the throwaway ssh folder; the other records are written
#   directly, in the state schema's shape.
# - Every document printed MUST validate against its output schema, and the state file against the
#   state schema after the run.
# - The credential step has no root override, so each ready lease's folder lands under the real
#   %USERPROFILE%\.agentspace\leases, and exactly those folders are deleted in a finally. The real
#   ~/.ssh config is hashed and ~/.ssh/agentspace.d listed before and after, and a change fails the
#   run. No key is printed. Nothing connects anywhere.
# It prints PASS or FAIL per check and exits 1 on any failure.
# Run: pwsh -NoProfile -File scripts\test-lease-cmd-renew-release-list.ps1

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$cli = Join-Path $repo 'scripts\lease-cli.ps1'
$pwsh = (Get-Process -Id $PID).Path
$leaseSchema = Join-Path $repo 'schemas\lease.output.schema.json'
$listSchema = Join-Path $repo 'schemas\lease-list.output.schema.json'
$errorSchema = Join-Path $repo 'schemas\error.output.schema.json'
$stateSchema = Join-Path $repo 'schemas\state\leases.state.schema.json'
$failures = 0
$passes = 0

. (Join-Path $PSScriptRoot 'lease-driver.ps1')

function Write-Result([bool]$Ok, [string]$Text) {
  if ($Ok) { Write-Host "PASS  $Text"; $script:passes++ } else { Write-Host "FAIL  $Text"; $script:failures++ }
}

function Test-Document([string]$Json, [string]$SchemaPath) {
  try { [bool](Test-Json -Json $Json -SchemaFile $SchemaPath -ErrorAction Stop) } catch { $false }
}

function Get-RealSshState {
  $ssh = Join-Path $env:USERPROFILE '.ssh'
  $cfg = Join-Path $ssh 'config'
  $c = if (Test-Path -LiteralPath $cfg) { (Get-FileHash -LiteralPath $cfg -Algorithm SHA256).Hash } else { 'absent' }
  $d = Join-Path $ssh 'agentspace.d'
  $names = if (Test-Path -LiteralPath $d) {
    (Get-ChildItem -LiteralPath $d -Force | Sort-Object Name | ForEach-Object { "$($_.Name):$($_.Length):$($_.LastWriteTimeUtc.Ticks)" }) -join '|'
  } else { 'absent' }
  "$c;$names"
}

function Format-Time([DateTime]$T) { $T.ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture) }
function Get-Instant([string]$Text) { [DateTime]::ParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal) }

# Runs the CLI; returns Out, Err and Code. Output is read as text; no key can appear in it.
function Invoke-Cli([string[]]$CliArgs) {
  $psi = [System.Diagnostics.ProcessStartInfo]::new($pwsh)
  foreach ($a in @('-NoProfile', '-File', $cli, 'lease') + $CliArgs) { $psi.ArgumentList.Add($a) }
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  $p = [System.Diagnostics.Process]::Start($psi)
  $so = $p.StandardOutput.ReadToEndAsync()
  $se = $p.StandardError.ReadToEndAsync()
  $p.WaitForExit()
  [pscustomobject]@{ Out = $so.Result; Err = $se.Result; Code = $p.ExitCode }
}

function Test-Success([string]$Name, $Run, [string]$Schema) {
  $ok = $Run.Code -eq 0 -and $Run.Err -eq '' -and $Run.Out.EndsWith("`n") -and (Test-Document $Run.Out.TrimEnd("`n") $Schema)
  Write-Result $ok "$Name exits 0 and validates"
  if (-not $ok) { Write-Host "      exit $($Run.Code), stderr: $($Run.Err.Trim())" }
  if ($ok) { $Run.Out | ConvertFrom-Json -AsHashtable -DateKind String }
}

function Test-Failure([string]$Name, $Run, [int]$Code, [string]$ErrCode, [string]$State = '', [string]$LeaseId = '') {
  $doc = $null
  try { $doc = $Run.Err.TrimEnd("`n") | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  $ok = $Run.Code -eq $Code -and $Run.Out -eq '' -and $doc -and $doc['code'] -ceq $ErrCode -and (Test-Document $Run.Err.TrimEnd("`n") $errorSchema)
  if ($ok -and $State) { $ok = $doc['state'] -ceq $State -and $doc['message'].Contains($State) }
  if ($ok -and $LeaseId -and $doc.ContainsKey('lease_id')) { $ok = $doc['lease_id'] -ceq $LeaseId }
  Write-Result $ok "$Name exits $Code as $ErrCode$(if ($State) { " naming $State" })"
  if (-not $ok) { Write-Host "      exit $($Run.Code), stdout: $($Run.Out.Trim()), stderr: $($Run.Err.Trim())" }
}

$stateDir = Join-Path $env:TEMP "agentspace-test-e105-state-$([guid]::NewGuid().ToString('N'))"
$sshDir = Join-Path $env:TEMP "agentspace-test-e105-ssh-$([guid]::NewGuid().ToString('N'))"
$leasesRoot = Join-Path $env:USERPROFILE '.agentspace\leases'
$madeIds = [System.Collections.Generic.List[string]]::new()
$realBefore = Get-RealSshState
$savedState = $env:AGENTSPACE_STATE_DIR
$savedSsh = $env:AGENTSPACE_SSH_DIR

function New-Id { [guid]::NewGuid().ToString() }

# A ready record with a real credential and alias in the throwaway ssh folder.
function New-ReadyRecord([string]$Id, [string]$Project, [string]$CreatedAt, [int]$TtlSeconds = 28800) {
  $script:madeIds.Add($Id)
  $r = Invoke-LeaseDriverPlace -LeaseId $Id -Project $Project -Platform 'linux/arm64' -Profile 'agent-linux' -Size 'm' `
    -Ttl ([TimeSpan]::FromSeconds($TtlSeconds)) -SshDir $sshDir -ProxyCommand 'C:\Windows\System32\cmd.exe /d /c exit 1'
  [ordered]@{
    id = $Id; project = $Project; state = 'ready'; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'
    ttl_seconds = $TtlSeconds; created_at = $CreatedAt; expires_at = (Format-Time $r.ExpiresAt)
    ssh = [ordered]@{ host_alias = $r.HostAlias; user = $r.User }; driver = $r.Driver
  }
}

function New-PlainRecord([string]$Id, [string]$Project, [string]$State, [string]$CreatedAt) {
  $r = [ordered]@{
    id = $Id; project = $Project; state = $State; platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'
    ttl_seconds = 3600; created_at = $CreatedAt
  }
  switch ($State) {
    'released' { $r.expires_at = Format-Time $now.AddMinutes(30); $r.released_at = Format-Time $now.AddMinutes(-30) }
    'unsatisfiable' {
      $r.platform = 'linux/amd64'
      $r.reasons = @([ordered]@{ code = 'unsupported_platform'; message = "Platform 'linux/amd64' is not supported; v0 supports linux/arm64 only." })
    }
  }
  $r
}

function Write-State($Records) {
  $text = ([ordered]@{ version = 0; leases = @($Records) } | ConvertTo-Json -Compress -Depth 20) + "`n"
  New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
  [System.IO.File]::WriteAllText((Join-Path $stateDir 'leases.json'), $text, [Text.UTF8Encoding]::new($false))
}

function Read-StateRecords {
  $doc = Get-Content -LiteralPath (Join-Path $stateDir 'leases.json') -Raw | ConvertFrom-Json -AsHashtable -DateKind String
  $doc['leases']
}

function Get-KeyHash([string]$Id) {
  $k = Join-Path $leasesRoot "$Id\id_ed25519"
  if (Test-Path -LiteralPath $k) { (Get-FileHash -LiteralPath $k -Algorithm SHA256).Hash } else { 'absent' }
}

function Get-CertPath([string]$Id) { Join-Path $leasesRoot "$Id\id_ed25519-cert.pub" }

function Get-CertHash([string]$Id) {
  $c = Get-CertPath $Id
  if (Test-Path -LiteralPath $c) { (Get-FileHash -LiteralPath $c -Algorithm SHA256).Hash } else { 'absent' }
}

# The certificate's valid-before as a UTC time text. ssh-keygen -L prints local time with no zone (D7).
function Get-CertValidBefore([string]$Id) {
  $out = & ssh-keygen -L -f (Get-CertPath $Id) 2>&1 | Out-String
  if ($out -match 'Valid:\s+from\s+\S+\s+to\s+(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})') {
    $local = [DateTime]::SpecifyKind([DateTime]::ParseExact($Matches[1], 'yyyy-MM-dd''T''HH:mm:ss', [Globalization.CultureInfo]::InvariantCulture), [DateTimeKind]::Local)
    Format-Time $local
  } else { 'unreadable' }
}

# The ids in the order the contract gives: created_at, then id, both ordinal.
function Get-ExpectedOrder($Records) {
  $list = [System.Collections.Generic.List[object]]::new()
  foreach ($r in $Records) { $list.Add($r) }
  $list.Sort([Comparison[object]]{
    param($a, $b)
    $c = [string]::CompareOrdinal($a['created_at'], $b['created_at'])
    if ($c -eq 0) { $c = [string]::CompareOrdinal($a['id'], $b['id']) }
    $c
  })
  @($list | ForEach-Object { $_['id'] })
}

try {
  $env:AGENTSPACE_STATE_DIR = $stateDir
  $env:AGENTSPACE_SSH_DIR = $sshDir
  New-Item -ItemType Directory -Force -Path $sshDir | Out-Null

  # --- Seed -------------------------------------------------------------------------------------
  $now = [DateTime]::UtcNow
  $idRenew = New-Id; $idTtl = New-Id; $idRelease = New-Id; $idExpiredRel = New-Id
  $idExpired = New-Id; $idReleased = New-Id; $idUnsat = New-Id; $idPending = New-Id; $idPendingRel = New-Id; $idUnknown = New-Id
  $t = { param($min) Format-Time $now.AddMinutes($min) }
  $rRenew = New-ReadyRecord $idRenew 'alpha' (& $t -50)
  $rTtl = New-ReadyRecord $idTtl 'alpha' (& $t -49)
  $rRelease = New-ReadyRecord $idRelease 'beta' (& $t -48)
  $rExpiredRel = New-ReadyRecord $idExpiredRel 'beta' (& $t -47)
  $rExpiredRel.expires_at = & $t -5
  $rExpired = New-ReadyRecord $idExpired 'alpha' (& $t -46)
  $rExpired.expires_at = & $t -1
  $rReleased = New-PlainRecord $idReleased 'alpha' 'released' (& $t -45)
  $rUnsat = New-PlainRecord $idUnsat 'gamma' 'unsatisfiable' (& $t -44)
  $rPending = New-PlainRecord $idPending 'gamma' 'pending' (& $t -43)
  # The two pending records share one created_at, so the id tiebreak decides their order.
  $rPendingRel = New-PlainRecord $idPendingRel 'gamma' 'pending' (& $t -43)
  $records = @($rRenew, $rTtl, $rRelease, $rExpiredRel, $rExpired, $rReleased, $rUnsat, $rPending, $rPendingRel)
  $expectedOrder = Get-ExpectedOrder $records
  # The file holds them shuffled, with the higher id of the tied pair first, so no order in the file
  # matches the expected one by accident.
  $byId = @{}
  foreach ($r in $records) { $byId[$r['id']] = $r }
  $perm = @(4, 0, 7, 2, 8, 5, 1, 6, 3)
  $fileOrder = @($perm | ForEach-Object { $expectedOrder[$_] })
  $tied = if ([string]::CompareOrdinal($idPending, $idPendingRel) -lt 0) { @($idPending, $idPendingRel) } else { @($idPendingRel, $idPending) }
  $iLow = [array]::IndexOf($fileOrder, $tied[0]); $iHigh = [array]::IndexOf($fileOrder, $tied[1])
  if ($iLow -lt $iHigh) { $fileOrder[$iLow] = $tied[1]; $fileOrder[$iHigh] = $tied[0] }
  Write-State @($fileOrder | ForEach-Object { $byId[$_] })
  $shuffled = (@($fileOrder) -join ',') -cne ($expectedOrder -join ',') -and [array]::IndexOf($fileOrder, $tied[1]) -lt [array]::IndexOf($fileOrder, $tied[0])
  Write-Result $shuffled 'The seeded state file is out of order, with the tied pair in reverse id order'
  Write-Result (Test-Document (Get-Content -LiteralPath (Join-Path $stateDir 'leases.json') -Raw) $stateSchema) 'The seeded state file validates against the state schema'
  $aliasFiles = (Get-ChildItem -LiteralPath (Join-Path $sshDir 'agentspace.d') -ErrorAction SilentlyContinue).Count
  Write-Result ($aliasFiles -eq 5) "Five ready leases have an alias file in the throwaway ssh folder (found $aliasFiles)"

  # --- list ---------------------------------------------------------------------------------------
  # Run before any change, so the past-expiry lease is still stored as ready.
  $run = Invoke-Cli @('list')
  $doc = Test-Success 'list with no filter' $run $listSchema
  if ($doc) {
    Write-Result ($doc['leases'].Count -eq 9) "list with no filter returns all nine leases (found $($doc['leases'].Count))"
    $gotOrder = @($doc['leases'] | ForEach-Object { $_['id'] })
    Write-Result (($gotOrder -join ',') -ceq ($expectedOrder -join ',')) 'list returns the exact order: created_at, then id'
    $exp = $doc['leases'] | Where-Object { $_['id'] -ceq $idExpired }
    Write-Result ($exp -and $exp['state'] -ceq 'expired' -and -not $exp.ContainsKey('ssh')) 'list shows the past-expiry lease as expired, without ssh'
    Write-Result (-not ($run.Out -match 'driver|"ec2"')) 'list prints no driver name'
  }
  $doc = Test-Success 'list --project alpha' (Invoke-Cli @('list', '--project', 'alpha')) $listSchema
  if ($doc) { Write-Result ($doc['leases'].Count -eq 4 -and -not ($doc['leases'] | Where-Object { $_['project'] -cne 'alpha' })) 'list --project alpha returns only the four alpha leases' }
  $doc = Test-Success 'list --state ready' (Invoke-Cli @('list', '--state', 'ready')) $listSchema
  if ($doc) {
    $ids = @($doc['leases'] | ForEach-Object { $_['id'] })
    Write-Result ($ids.Count -eq 3 -and $ids -notcontains $idExpired -and -not ($doc['leases'] | Where-Object { $_['state'] -cne 'ready' })) 'list --state ready leaves out the past-expiry lease'
  }
  $doc = Test-Success 'list --state expired' (Invoke-Cli @('list', '--state', 'expired')) $listSchema
  if ($doc) { Write-Result ($doc['leases'].Count -eq 2 -and $doc['leases'][0]['id'] -ceq $idExpiredRel -and $doc['leases'][1]['id'] -ceq $idExpired) 'list --state expired returns the two past-expiry leases in order' }
  $doc = Test-Success 'list --state pending,released --project gamma' (Invoke-Cli @('list', '--state', 'pending,released', '--project', 'gamma')) $listSchema
  if ($doc) { Write-Result ($doc['leases'].Count -eq 2 -and -not ($doc['leases'] | Where-Object { $_['state'] -cne 'pending' })) 'list with two filters returns the two gamma pending leases' }
  $run = Invoke-Cli @('list', '--project', 'nomatch')
  $doc = Test-Success 'list with no match' $run $listSchema
  Write-Result ($run.Out -ceq "{`"leases`":[]}`n") 'list with no match prints {"leases":[]}'
  $one = Invoke-Cli @('list', '--project', 'alpha', '--state', 'released')
  $doc = Test-Success 'list with one match' $one $listSchema
  Write-Result ($one.Out.StartsWith('{"leases":[{')) 'list with one match keeps leases an array'

  # --- renew --------------------------------------------------------------------------------------
  $keyBefore = Get-KeyHash $idRenew
  $certBefore = Get-CertHash $idRenew
  $aliasBefore = $rRenew.ssh.host_alias
  $startedAt = [DateTime]::UtcNow
  $doc = Test-Success 'renew of a ready lease' (Invoke-Cli @('renew', $idRenew)) $leaseSchema
  if ($doc) {
    $certAfter = Get-CertHash $idRenew
    Write-Result ($certBefore -cne 'absent' -and $certAfter -cne 'absent' -and $certAfter -cne $certBefore) 'renew replaced the certificate file'
    $validBefore = Get-CertValidBefore $idRenew
    Write-Result ($validBefore -ceq $doc['expires_at']) "The renewed certificate's valid-before equals expires_at (certificate $validBefore, document $($doc['expires_at']))"
    $exp = Get-Instant $doc['expires_at']
    $delta = ($exp - $startedAt).TotalSeconds
    Write-Result ($delta -gt 28800 - 30 -and $delta -lt 28800 + 60) "renew sets expires_at to about now plus the TTL (off by $([int]($delta - 28800)) s)"
    Write-Result ($doc['id'] -ceq $idRenew -and $doc['ssh']['host_alias'] -ceq $aliasBefore -and $doc['state'] -ceq 'ready') 'renew keeps the ID and alias and stays ready'
    Write-Result ($doc['ttl_seconds'] -eq 28800) 'renew without --ttl reuses ttl_seconds'
    Write-Result ((Get-KeyHash $idRenew) -ceq $keyBefore -and $keyBefore -cne 'absent') 'renew leaves the private key byte-identical'
    $stored = Read-StateRecords | Where-Object { $_['id'] -ceq $idRenew }
    Write-Result ($stored['expires_at'] -ceq $doc['expires_at'] -and $stored['driver']) 'renew saved the new expires_at in the state file and kept the driver'
  }
  $certTtlBefore = Get-CertHash $idTtl
  $startedAt = [DateTime]::UtcNow
  $doc = Test-Success 'renew --ttl 90m' (Invoke-Cli @('renew', $idTtl, '--ttl', '90m')) $leaseSchema
  if ($doc) {
    Write-Result ((Get-CertHash $idTtl) -cne $certTtlBefore -and (Get-CertValidBefore $idTtl) -ceq $doc['expires_at']) 'renew --ttl 90m replaced the certificate, whose valid-before equals expires_at'
    $delta = ((Get-Instant $doc['expires_at']) - $startedAt).TotalSeconds
    Write-Result ($doc['ttl_seconds'] -eq 5400 -and $delta -gt 5400 - 30 -and $delta -lt 5400 + 60) 'renew --ttl 90m sets ttl_seconds 5400 and a matching expires_at'
    $doc2 = Test-Success 'renew again without --ttl' (Invoke-Cli @('renew', $idTtl)) $leaseSchema
    if ($doc2) { Write-Result ($doc2['ttl_seconds'] -eq 5400) 'a second renew reuses the 5400 s TTL' }
  }
  Test-Failure 'renew of an expired lease' (Invoke-Cli @('renew', $idExpired)) 4 invalid_state 'expired' $idExpired
  Test-Failure 'renew of a released lease' (Invoke-Cli @('renew', $idReleased)) 4 invalid_state 'released' $idReleased
  Test-Failure 'renew of an unsatisfiable lease' (Invoke-Cli @('renew', $idUnsat)) 4 invalid_state 'unsatisfiable' $idUnsat
  Test-Failure 'renew of a pending lease' (Invoke-Cli @('renew', $idPending)) 4 invalid_state 'pending' $idPending
  Test-Failure 'renew of an unknown lease' (Invoke-Cli @('renew', $idUnknown)) 3 not_found '' $idUnknown
  $stillExpired = Read-StateRecords | Where-Object { $_['id'] -ceq $idExpired }
  Write-Result ($stillExpired['expires_at'] -ceq $rExpired.expires_at) 'A refused renew leaves the record as it was'

  # --- release ------------------------------------------------------------------------------------
  $aliasFile = Join-Path $sshDir "agentspace.d\$($rRelease.ssh.host_alias)"
  $aliasFileFound = @(Get-ChildItem -LiteralPath (Join-Path $sshDir 'agentspace.d') | Where-Object { $_.Name -like "*$($idRelease.Substring(0, 8))*" }).Count
  Write-Result ($aliasFileFound -ge 1 -and (Test-Path -LiteralPath (Join-Path $leasesRoot $idRelease))) 'Before release the lease has a folder and an alias file'
  $run = Invoke-Cli @('release', $idRelease)
  $doc = Test-Success 'release of a ready lease' $run $leaseSchema
  if ($doc) {
    Write-Result ($doc['state'] -ceq 'released' -and $doc['released_at'] -and -not $doc.ContainsKey('ssh') -and $doc['expires_at'] -ceq $rRelease.expires_at) 'release gives state released with released_at, no ssh, and expires_at kept'
    Write-Result (-not (Test-Path -LiteralPath (Join-Path $leasesRoot $idRelease))) 'release leaves no lease folder, so no key or certificate'
    $left = @(Get-ChildItem -LiteralPath (Join-Path $sshDir 'agentspace.d') -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*$($idRelease.Substring(0, 8))*" }).Count
    Write-Result ($left -eq 0) 'release leaves no alias file'
    $first = $doc['released_at']
    Start-Sleep -Seconds 2
    $doc2 = Test-Success 'second release' (Invoke-Cli @('release', $idRelease)) $leaseSchema
    if ($doc2) { Write-Result ($doc2['released_at'] -ceq $first -and $doc2['state'] -ceq 'released') 'A second release returns the same released_at unchanged' }
    $stored = Read-StateRecords | Where-Object { $_['id'] -ceq $idRelease }
    Write-Result ($stored['state'] -ceq 'released' -and -not $stored.ContainsKey('ssh')) 'The state file holds the released record without ssh'
  }
  $doc = Test-Success 'release of an expired lease' (Invoke-Cli @('release', $idExpiredRel)) $leaseSchema
  if ($doc) {
    Write-Result ($doc['state'] -ceq 'released' -and -not (Test-Path -LiteralPath (Join-Path $leasesRoot $idExpiredRel))) 'release of an expired lease releases it and deletes its folder'
    $leftExp = @(Get-ChildItem -LiteralPath (Join-Path $sshDir 'agentspace.d') -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*$($idExpiredRel.Substring(0, 8))*" }).Count
    Write-Result ($leftExp -eq 0) 'release of an expired lease deletes its alias file'
    Write-Result ($doc['expires_at'] -ceq $rExpiredRel.expires_at) 'release of an expired lease keeps its expires_at'
  }
  $doc = Test-Success 'release of a pending lease' (Invoke-Cli @('release', $idPendingRel)) $leaseSchema
  if ($doc) { Write-Result ($doc['state'] -ceq 'released' -and $doc['released_at'] -and -not $doc.ContainsKey('expires_at')) 'release of a pending lease gives released with released_at and no expires_at' }
  $run = Invoke-Cli @('release', $idUnsat)
  $doc = Test-Success 'release of an unsatisfiable lease' $run $leaseSchema
  if ($doc) { Write-Result ($doc['state'] -ceq 'unsatisfiable' -and $doc['reasons'].Count -eq 1 -and -not $doc.ContainsKey('released_at')) 'release of an unsatisfiable lease returns it unchanged' }
  Test-Failure 'release of an unknown lease' (Invoke-Cli @('release', $idUnknown)) 3 not_found '' $idUnknown

  # --- After ----------------------------------------------------------------------------------------
  $text = Get-Content -LiteralPath (Join-Path $stateDir 'leases.json') -Raw
  Write-Result (Test-Document $text $stateSchema) 'The state file validates against the state schema after the run'
  $allRun = Invoke-Cli @('list')
  $doc = Test-Success 'list after the changes' $allRun $listSchema
  if ($doc) {
    $released = @($doc['leases'] | Where-Object { $_['state'] -ceq 'released' }).Count
    Write-Result ($released -eq 4) "list after the run shows four released leases (found $released)"
  }
}
finally {
  $env:AGENTSPACE_STATE_DIR = $savedState
  $env:AGENTSPACE_SSH_DIR = $savedSsh
  foreach ($id in $madeIds) {
    $d = Join-Path $leasesRoot $id
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }
  }
  foreach ($d in @($stateDir, $sshDir)) {
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }
  }
  $leftover = @($madeIds | Where-Object { Test-Path -LiteralPath (Join-Path $leasesRoot $_) }).Count
  Write-Result ($leftover -eq 0) 'Every lease folder the test made is removed'
  Write-Result ((Get-RealSshState) -ceq $realBefore) "The real ~/.ssh config and agentspace.d are unchanged"
}

Write-Output "$passes passed, $failures failed"
if ($failures -gt 0) { exit 1 }
exit 0

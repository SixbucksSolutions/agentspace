#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for `agentspace lease create` and `agentspace lease get` (story E1.04), run through
# the real entry point as a child process: pwsh -NoProfile -File scripts\lease-cli.ps1 lease ...
# - The state file and the ssh folder are throwaway folders under $env:TEMP, named by
#   AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR; AGENTSPACE_PROXY_COMMAND is a harmless string.
#   Nothing connects anywhere.
# - The credential step has no root override and uses the real CA, so every lease it creates lands
#   under the real %USERPROFILE%\.agentspace\leases. Each such folder is deleted in a finally.
# - Terry's real ~/.ssh config is hashed and its agentspace.d listed before and after; any change
#   fails the run. No key is ever printed.
# It prints PASS or FAIL per check and exits 1 on any failure.
# Run: pwsh -NoProfile -File scripts\test-lease-cmd-create-get.ps1

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$cli = Join-Path $repo 'scripts\lease-cli.ps1'
$pwsh = (Get-Process -Id $PID).Path
$leaseSchema = Join-Path $repo 'schemas\lease.output.schema.json'
$errorSchema = Join-Path $repo 'schemas\error.output.schema.json'
$stateSchema = Join-Path $repo 'schemas\state\leases.state.schema.json'
$failures = 0
$passes = 0

function Write-Result([bool]$Ok, [string]$Text) {
  if ($Ok) { Write-Output "PASS  $Text"; $script:passes++ } else { Write-Output "FAIL  $Text"; $script:failures++ }
}

function Test-Valid([string]$Json, [string]$SchemaPath) {
  try { [bool](Test-Json -Json $Json -SchemaFile $SchemaPath -ErrorAction Stop) } catch { $false }
}

# A hash of the real ~/.ssh config and a listing of its alias folder.
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

function Invoke-Lease([string[]]$Arguments) {
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $pwsh
  foreach ($a in '-NoProfile', '-File', $cli, 'lease') { $psi.ArgumentList.Add($a) }
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $p = [Diagnostics.Process]::Start($psi)
  $so = $p.StandardOutput.ReadToEndAsync()
  $se = $p.StandardError.ReadToEndAsync()
  $p.WaitForExit()
  [pscustomobject]@{ Exit = $p.ExitCode; Out = $so.Result.TrimEnd("`n"); Err = $se.Result.TrimEnd("`n") }
}

$temp = Join-Path $env:TEMP "agentspace-test-create-get-$([guid]::NewGuid().ToString('N'))"
$stateDir = Join-Path $temp 'state'
$sshDir = Join-Path $temp 'ssh'
$statePath = Join-Path $stateDir 'leases.json'
$leasesRoot = Join-Path $env:USERPROFILE '.agentspace\leases'
$before = if (Test-Path -LiteralPath $leasesRoot) { @(Get-ChildItem -LiteralPath $leasesRoot -Directory | ForEach-Object Name) } else { @() }
$realBefore = Get-RealSshState
$env:AGENTSPACE_STATE_DIR = $stateDir
$env:AGENTSPACE_SSH_DIR = $sshDir
$env:AGENTSPACE_PROXY_COMMAND = 'C:\Windows\System32\cmd.exe /d /c exit 1'
$created = [System.Collections.Generic.List[string]]::new()
$uuid4 = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'
try {
  # A default create is ready.
  $r = Invoke-Lease @('create', '--project', 'Test-E104')
  $a = $null
  try { $a = $r.Out | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  if ($a -and $a['id']) { $created.Add($a['id']) }
  Write-Result ($r.Exit -eq 0 -and $r.Err -eq '') 'A default create exits 0 with nothing on stderr'
  Write-Result ($a -and $a['state'] -ceq 'ready') 'A default create is ready'
  Write-Result (Test-Valid $r.Out $leaseSchema) 'A default create validates against lease.output.schema.json'
  Write-Result ($a -and $a['id'] -cmatch $uuid4) 'The ID is a lowercase UUIDv4'
  Write-Result ($a -and $a['project'] -ceq 'test-e104' -and $a['platform'] -ceq 'linux/arm64' -and $a['profile'] -ceq 'agent-linux' -and $a['size'] -ceq 'm') 'The defaults and the lowercased project are filled in'
  $span = if ($a) { ([DateTime]::Parse($a['expires_at']).ToUniversalTime() - [DateTime]::Parse($a['created_at']).ToUniversalTime()).TotalSeconds } else { -1 }
  Write-Result ($a -and $a['ttl_seconds'] -eq 28800 -and [Math]::Abs($span - 28800) -le 2) 'Expires_at minus created_at is the 8 hour TTL'
  Write-Result ($a -and -not $r.Out.Contains('driver')) 'The driver name is not in the output'
  $alias = if ($a) { $a['ssh']['host_alias'] } else { '' }
  Write-Result ($a -and $alias -ceq "agentspace-test-e104-$($a['id'].Substring(0, 8))" -and $a['ssh']['user'] -ceq 'ubuntu') 'The ssh block names the alias and the user'
  $aliasFile = Join-Path $sshDir "agentspace.d\$alias"
  Write-Result ($a -and (Test-Path -LiteralPath $aliasFile) -and ((Get-Content -LiteralPath $aliasFile -Raw) -match [regex]::Escape($alias))) 'The alias file exists in the throwaway agentspace.d and names the lease'
  $leaseDir = if ($a) { Join-Path $leasesRoot $a['id'] } else { Join-Path $leasesRoot 'none' }
  Write-Result ((Test-Path -LiteralPath (Join-Path $leaseDir 'id_ed25519')) -and (Test-Path -LiteralPath (Join-Path $leaseDir 'id_ed25519-cert.pub'))) 'The key and certificate exist under the lease folder'

  # TTL.
  $r2 = Invoke-Lease @('create', '--project', 'test-e104', '--ttl', '90m')
  $b = $null
  try { $b = $r2.Out | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  if ($b -and $b['id']) { $created.Add($b['id']) }
  $span2 = if ($b) { ([DateTime]::Parse($b['expires_at']).ToUniversalTime() - [DateTime]::Parse($b['created_at']).ToUniversalTime()).TotalSeconds } else { -1 }
  Write-Result ($r2.Exit -eq 0 -and $b -and $b['ttl_seconds'] -eq 5400 -and [Math]::Abs($span2 - 5400) -le 2) '--ttl 90m gives 5400 seconds'
  Write-Result (Test-Valid $r2.Out $leaseSchema) 'The 90m lease validates against lease.output.schema.json'
  Write-Result ($a -and $b -and $a['id'] -cne $b['id'] -and $a['ssh']['host_alias'] -cne $b['ssh']['host_alias']) 'Two creates get different IDs and aliases'

  # get, with the ID in uppercase.
  $g = Invoke-Lease @('get', $a['id'].ToUpperInvariant())
  Write-Result ($g.Exit -eq 0 -and $g.Out -ceq $r.Out) 'get with the ID in uppercase prints the same lease'
  Write-Result (Test-Valid $g.Out $leaseSchema) 'get validates against lease.output.schema.json'

  # Unsatisfiable.
  $u = Invoke-Lease @('create', '--project', 'test-e104', '--platform', 'linux/amd64', '--size', 'xl')
  $ua = $null
  try { $ua = $u.Out | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  if ($ua -and $ua['id']) { $created.Add($ua['id']) }
  Write-Result ($u.Exit -eq 0 -and $ua -and $ua['state'] -ceq 'unsatisfiable') 'An unsatisfiable create exits 0 with state unsatisfiable'
  Write-Result ($ua -and @($ua['reasons']).Count -eq 2 -and $ua['reasons'][0]['code'] -ceq 'unsupported_platform' -and $ua['reasons'][1]['code'] -ceq 'unsupported_size') 'Its two reasons come in the contract order'
  Write-Result (Test-Valid $u.Out $leaseSchema) 'The unsatisfiable lease validates against lease.output.schema.json'
  $uAlias = if ($ua) { "agentspace-test-e104-$($ua['id'].Substring(0, 8))" } else { 'none' }
  Write-Result ($ua -and -not (Test-Path -LiteralPath (Join-Path $leasesRoot $ua['id'])) -and -not (Test-Path -LiteralPath (Join-Path $sshDir "agentspace.d\$uAlias"))) 'It leaves no lease folder and no alias file'
  $ug = Invoke-Lease @('get', $ua['id'])
  Write-Result ($ug.Exit -eq 0 -and $ug.Out -ceq $u.Out) 'get returns the unsatisfiable lease as create printed it'

  # not_found.
  $unknown = [guid]::NewGuid().ToString()
  $nf = Invoke-Lease @('get', $unknown)
  $nfd = $null
  try { $nfd = $nf.Err | ConvertFrom-Json -AsHashtable } catch { }
  Write-Result ($nf.Exit -eq 3 -and $nf.Out -eq '') 'get of an unknown ID exits 3 with nothing on stdout'
  Write-Result ($nfd -and $nfd['code'] -ceq 'not_found' -and $nfd['lease_id'] -ceq $unknown) 'The error is not_found and carries lease_id'
  Write-Result (Test-Valid $nf.Err $errorSchema) 'The not_found error validates against error.output.schema.json'

  # A ready lease whose expiry has passed reads as expired.
  $doc = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable -DateKind String -Depth 20
  foreach ($rec in $doc['leases']) { if ($rec['id'] -ceq $a['id']) { $rec['expires_at'] = '2020-01-01T00:00:00Z' } }
  [IO.File]::WriteAllText($statePath, (($doc | ConvertTo-Json -Compress -Depth 20) + "`n"))
  $e = Invoke-Lease @('get', $a['id'])
  $ea = $null
  try { $ea = $e.Out | ConvertFrom-Json -AsHashtable -DateKind String } catch { }
  Write-Result ($e.Exit -eq 0 -and $ea -and $ea['state'] -ceq 'expired' -and -not $ea.ContainsKey('ssh')) 'A ready lease past its expiry reads as expired, without ssh'
  Write-Result (Test-Valid $e.Out $leaseSchema) 'The expired lease validates against lease.output.schema.json'

  # The state file.
  Write-Result (Test-Valid (Get-Content -LiteralPath $statePath -Raw) $stateSchema) 'The state file validates against the state schema'
  $final = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable -DateKind String -Depth 20
  Write-Result (@($final['leases']).Count -eq 3) 'The state file holds the three leases that create made'
} catch {
  Write-Result $false "No unexpected error: $($_.Exception.Message)"
} finally {
  foreach ($id in $created) {
    $d = Join-Path $leasesRoot $id
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
  }
  # Anything else the run left under the real leases root, such as a folder from a failed child.
  if (Test-Path -LiteralPath $leasesRoot) {
    foreach ($d in Get-ChildItem -LiteralPath $leasesRoot -Directory) {
      if ($before -cnotcontains $d.Name) { Remove-Item -LiteralPath $d.FullName -Recurse -Force }
    }
  }
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
}
Write-Result ((Get-RealSshState) -ceq $realBefore) 'The real ~/.ssh config and agentspace.d are unchanged'
$after = if (Test-Path -LiteralPath $leasesRoot) { @(Get-ChildItem -LiteralPath $leasesRoot -Directory | ForEach-Object Name) } else { @() }
Write-Result ((($after | Sort-Object) -join ',') -ceq (($before | Sort-Object) -join ',')) 'The real leases folder is as it was'
Write-Output "$passes passed, $failures failed."
if ($failures) { exit 1 }

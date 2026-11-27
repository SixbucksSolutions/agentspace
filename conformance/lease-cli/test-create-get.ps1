#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# A conformance check (D34): `agentspace lease create` and `agentspace lease get`. It runs the
# program AGENTSPACE_CMD names, as a separate process, in a throwaway home with a throwaway CA
# (conformance/lease-cli/harness.ps1), and reads only files that docs/state.md names: the state file,
# the alias file and the lease folder, plus ssh-keygen, a program outside agentspace, on the key and
# the certificate. The expiry of a lease is read by seeding times in the state file, since the check
# has no clock of the program's.
# Checks that need a ready lease are reported SKIPPED when no throwaway CA could be made; a run with
# any SKIPPED is incomplete, never passed. The real CA, ~/.ssh and %USERPROFILE%\.agentspace are never
# touched, and no prototype code is loaded.
# Run: pwsh -NoProfile -File conformance/lease-cli/test-create-get.ps1
# Exits 0 when every check passed, 1 when any failed, 2 when none failed but any was skipped.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'harness.ps1')
. (Join-Path $PSScriptRoot 'lease-cli-common.ps1')

$uuid4 = '\A[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z'

function Get-LeaseFolderNames {
  param($ConformanceHome)
  $root = Join-Path $ConformanceHome.UserProfile '.agentspace\leases'
  if (-not (Test-Path -LiteralPath $root)) { return @() }
  return @(Get-ChildItem -LiteralPath $root -Force | ForEach-Object Name)
}

function Get-AliasFile {
  param($ConformanceHome, [string]$Alias, [string]$SshDir)
  if (-not $SshDir) { $SshDir = $ConformanceHome.SshDir }
  return (Assert-ConformancePath -ConformanceHome $ConformanceHome -Path (Join-Path $SshDir "agentspace.d\$Alias"))
}

# A canonical text of a parsed JSON value: sorted keys, so two documents compare by content and never
# by member order or by how a string is escaped (lease-api.md, "Output": a caller MUST compare parsed
# values, never bytes).
function ConvertTo-CanonicalJson {
  param($Value)
  if ($null -eq $Value) { return 'null' }
  if ($Value -is [System.Collections.IDictionary]) {
    $keys = @($Value.Keys | ForEach-Object { [string]$_ })
    [array]::Sort($keys, [StringComparer]::Ordinal)
    $parts = foreach ($k in $keys) { (ConvertTo-Json -InputObject $k -Compress) + ':' + (ConvertTo-CanonicalJson $Value[$k]) }
    return '{' + (@($parts) -join ',') + '}'
  }
  if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
    $items = foreach ($i in $Value) { ConvertTo-CanonicalJson $i }
    return '[' + (@($items) -join ',') + ']'
  }
  return (ConvertTo-Json -InputObject $Value -Compress)
}

# True when two stdouts hold the same JSON document, compared as parsed values, every time kept as a string.
function Test-SameDocument {
  param([string]$A, [string]$B)
  try {
    $da = $A | ConvertFrom-Json -AsHashtable -DateKind String -ErrorAction Stop
    $db = $B | ConvertFrom-Json -AsHashtable -DateKind String -ErrorAction Stop
  } catch { return $false }
  if ($null -eq $da -or $null -eq $db) { return $false }
  return ((ConvertTo-CanonicalJson $da) -ceq (ConvertTo-CanonicalJson $db))
}

# Runs a ready create and the checks every ready lease MUST meet; returns the run and the clock window.
function Test-ReadyCreate {
  param($ConformanceHome, [string[]]$Arguments, [string]$Label, [int]$Ttl, [string]$ExpectProject)
  $t0 = Get-ConformanceNow
  $r = Invoke-AgentspaceCli -ConformanceHome $ConformanceHome -Arguments $Arguments
  $t1 = Get-ConformanceNow
  $a = $r.Json
  $p = Get-ConformanceSuccessProblems -Run $r
  if ($a -and $a['state'] -cne 'ready') { $p.Add("State is '$($a['state'])', not 'ready'") }
  if ($a -and $a['ttl_seconds'] -ne $Ttl) { $p.Add("ttl_seconds is $($a['ttl_seconds']), not $Ttl") }
  Complete-ConformanceCheck -Name "lease-api.md#agentspace-lease-create: $Label is ready, exit 0, one compact document, validates against lease.output.schema.json" -Problems $p
  # A path is built from the document's id and alias only when the document validated: the program
  # under test is untrusted (docs/conformance.md).
  $valid = ($p.Count -eq 0)

  # The times are the suite's own clock's (lease-api.md#times): whole seconds in UTC, created_at during the
  # command, and expires_at the issue time plus the TTL, the issue time also during the command.
  $p = [System.Collections.Generic.List[string]]::new()
  if ($a -and $a['created_at'] -and $a['expires_at']) {
    $created = ConvertFrom-ConformanceTime $a['created_at']
    $expires = ConvertFrom-ConformanceTime $a['expires_at']
    if (-not $created -or -not $expires) { $p.Add('A time is not YYYY-MM-DDTHH:MM:SSZ') }
    else {
      if ($created -lt $t0 -or $created -gt $t1) { $p.Add("created_at $($a['created_at']) is outside the command's own window $($t0.ToString('HH:mm:ss'))Z to $($t1.ToString('HH:mm:ss'))Z") }
      $issued = $expires.AddSeconds(-$Ttl)
      if ($issued -lt $t0 -or $issued -gt $t1) { $p.Add("expires_at $($a['expires_at']) minus the TTL is outside the command's own window") }
    }
  } else { $p.Add('No created_at or expires_at') }
  Complete-ConformanceCheck -Name "lease-api.md#times: $Label has created_at and expires_at in UTC within the command's window, and expires_at is the issue time plus the TTL" -Problems $p

  $p = [System.Collections.Generic.List[string]]::new()
  if ($a) {
    if ($a['id'] -cnotmatch $uuid4) { $p.Add("The id '$($a['id'])' is not a lowercase UUIDv4") }
    if ($a['project'] -cne $ExpectProject) { $p.Add("project is '$($a['project'])', not '$ExpectProject'") }
    if ($a['platform'] -cne 'linux/arm64' -or $a['profile'] -cne 'agent-linux' -or $a['size'] -cne 'm') { $p.Add('A default was not filled in (linux/arm64, agent-linux, m)') }
    $alias = if ($a['ssh']) { $a['ssh']['host_alias'] } else { '' }
    if ($a['id'] -and $alias -cne "agentspace-$ExpectProject-$($a['id'].Substring(0, 8))") { $p.Add("host_alias is '$alias'") }
    if (-not $a['ssh'] -or $a['ssh']['user'] -cne 'ubuntu') { $p.Add('ssh.user is not ubuntu') }
    if ($r.Stdout -match '(?i)\b(ec2|aws|driver)\b') { $p.Add('The output names a backend or a driver') }
  } else { $p.Add('No document') }
  Complete-ConformanceCheck -Name "lease-api.md#fields: $Label has a lowercase UUIDv4 id, the lowercased project, the defaults, the alias agentspace-<project>-<first 8 hex> and user ubuntu, and names no backend" -Problems $p
  return [pscustomobject]@{ Run = $r; Doc = $a; Valid = $valid; T0 = $t0; T1 = $t1 }
}

$h = $null
try {
  $h = New-ConformanceHome
  # Under D54 a create is ready only for a project with a ready host (E0.07-T04-S01): the one project
  # every ready create below uses, test-create, gets one. The unsupported creates below never place.
  [void](Add-ConformanceReadyHost -ConformanceHome $h -Project 'test-create')
  $made = 0                       # Records that creates recorded, to compare with the state file's.
  $badSkip = 'No throwaway CA could be made: ' + $h.CaReason

  # --- A ready lease -------------------------------------------------------------------------------
  $ready = $null
  if ($h.CaReady) {
    $ready = Test-ReadyCreate -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'Test-Create') -Label 'A default create' -Ttl 28800 -ExpectProject 'test-create'
    $made++
    $a = $ready.Doc
    $id = if ($a -and $ready.Valid -and $a['id'] -cmatch $uuid4) { [string]$a['id'] } else { '00000000-0000-4000-8000-000000000000' }
    $alias = if ($a -and $ready.Valid -and $a['ssh'] -and [string]$a['ssh']['host_alias'] -cmatch '\Aagentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}\z') { [string]$a['ssh']['host_alias'] } else { 'agentspace-none-00000000' }

    # The alias file (state.md#the-alias-file): in <ssh folder>\agentspace.d, owner mark first, then the Host block.
    $p = [System.Collections.Generic.List[string]]::new()
    $aliasFile = Get-AliasFile -ConformanceHome $h -Alias $alias
    if (-not (Test-Path -LiteralPath $aliasFile -PathType Leaf)) { $p.Add('The alias file is not in the ssh folder\agentspace.d') }
    else {
      $lines = ([IO.File]::ReadAllText($aliasFile) -split "`n")
      if ($lines[0] -cnotlike "# agentspace lease ${id};*") { $p.Add('The first line is not the owner mark naming the lease') }
      if ($lines[1] -cne "Host $alias") { $p.Add("The second line is not 'Host $alias'") }
    }
    Complete-ConformanceCheck -Name 'state.md#the-alias-file: A ready create writes <ssh folder>\agentspace.d\<alias>, starting with the lease owner mark and the Host line' -Problems $p

    # The lease folder (state.md#the-lease-folder): exactly the key pair and the certificate.
    $p = [System.Collections.Generic.List[string]]::new()
    $dir = Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $h.UserProfile ".agentspace\leases\$id")
    $names = if (Test-Path -LiteralPath $dir) { @(Get-ChildItem -LiteralPath $dir -Force | ForEach-Object Name | Sort-Object) } else { @() }
    $want = @('id_ed25519', 'id_ed25519-cert.pub', 'id_ed25519.pub')
    if (($names -join ',') -cne ($want -join ',')) { $p.Add("The lease folder under the home's .agentspace\leases holds '$($names -join ',')'") }
    Complete-ConformanceCheck -Name 'state.md#the-lease-folder: A ready create makes <home>\.agentspace\leases\<id> holding exactly id_ed25519, id_ed25519.pub and id_ed25519-cert.pub' -Problems $p

    # The key and the certificate, read with ssh-keygen (a program outside agentspace).
    $keygen = Get-Command ssh-keygen -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $keygen) {
      Write-ConformanceResult -Name 'state.md#the-key: The private key loads with an empty passphrase' -Status SKIPPED -Reason 'No ssh-keygen was found on the PATH'
      Write-ConformanceResult -Name 'state.md#the-certificate: The certificate is valid until expires_at, names the lease, and has the principal ubuntu' -Status SKIPPED -Reason 'No ssh-keygen was found on the PATH'
    } else {
      $childEnv = Get-ConformanceChildEnvironment -ConformanceHome $h
      $key = Join-Path $dir 'id_ed25519'
      $p = [System.Collections.Generic.List[string]]::new()
      $firstLine = if (Test-Path -LiteralPath $key) { [IO.File]::ReadAllLines($key)[0] } else { '' }
      if ($firstLine -cne '-----BEGIN OPENSSH PRIVATE KEY-----') { $p.Add('The private key is not in the OpenSSH private key format') }
      $y = Start-ConformanceProcess -FilePath $keygen.Source -Arguments @('-y', '-P', '', '-f', $key) -ChildEnvironment $childEnv
      $pub = if (Test-Path -LiteralPath "$key.pub") { ([IO.File]::ReadAllText("$key.pub").Trim() -split '\s+') } else { @() }
      if ($y.ExitCode -ne 0) { $p.Add('The key does not load with an empty passphrase') }
      elseif ($pub.Count -lt 2 -or ($y.Stdout.Trim() -split '\s+')[1] -cne $pub[1]) { $p.Add('The public key is not the private key''s own') }
      if ($ready.Run.Stdout + $ready.Run.Stderr -match 'PRIVATE KEY') { $p.Add('The private key is in the output') }
      Complete-ConformanceCheck -Name 'state.md#the-key: The private key is an unencrypted OpenSSH ed25519 key that loads with an empty passphrase, never appears in the output, and matches id_ed25519.pub' -Problems $p

      $p = [System.Collections.Generic.List[string]]::new()
      $l = Start-ConformanceProcess -FilePath $keygen.Source -Arguments @('-L', '-f', (Join-Path $dir 'id_ed25519-cert.pub')) -ChildEnvironment $childEnv
      $text = $l.Stdout -replace "`r", ''
      if ($l.ExitCode -ne 0) { $p.Add('ssh-keygen could not read the certificate') }
      else {
        # ssh-keygen -L prints local times with no zone, so they are converted before they are compared.
        $local = [Globalization.DateTimeStyles]::AssumeLocal
        if ($text -notmatch '(?m)^\s*Valid: from (\S+) to (\S+)\s*$') { $p.Add('No validity window') }
        else {
          $from = [DateTime]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture, $local).ToUniversalTime()
          $to = [DateTime]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture, $local).ToUniversalTime()
          $expires = ConvertFrom-ConformanceTime $a['expires_at']
          if ($to -ne $expires) { $p.Add("Valid-before is $($to.ToString('s'))Z, not expires_at $($a['expires_at'])") }
          if ($from -gt $expires.AddSeconds(-28800)) { $p.Add('Valid-after is later than the issue time') }
        }
        if ($text -notmatch '(?m)^\s*Key ID: "([^"]*)"\s*$' -or $Matches[1] -cne $id) { $p.Add('The key ID is not the lease ID') }
        if ($text -notmatch '(?ms)^\s*Principals:\s*\n\s*(\S+)\s*\n\s*Critical Options') { $p.Add('The principals could not be read') }
        elseif ($Matches[1] -cne 'ubuntu') { $p.Add("The principal is '$($Matches[1])', not ssh.user") }
      }
      Complete-ConformanceCheck -Name 'state.md#the-certificate: The certificate is valid until expires_at, is not valid-after its issue time, has the lease ID as key ID and the one principal ubuntu' -Problems $p
    }

    # get of the ready lease: the same document, with the ID in uppercase, and with spaces around it.
    $stateBefore = Get-ConformanceStateHash -ConformanceHome $h
    $g = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', $id.ToUpperInvariant())
    $p = Get-ConformanceSuccessProblems -Run $g
    if (-not (Test-SameDocument $g.Stdout $ready.Run.Stdout)) { $p.Add('get does not print the lease as create printed it, compared as parsed values') }
    Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-get: get with the ID in uppercase prints the lease as create printed it, and validates' -Problems $p
    $g2 = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', "  $id ")
    $p = Get-ConformanceSuccessProblems -Run $g2
    if (-not (Test-SameDocument $g2.Stdout $ready.Run.Stdout)) { $p.Add('get does not print the same lease, compared as parsed values') }
    Complete-ConformanceCheck -Name 'lease-api.md#lease-id: get takes an ID with surrounding whitespace, normalized' -Problems $p
    $p = [System.Collections.Generic.List[string]]::new()
    if ($stateBefore -ceq 'absent') { $p.Add('There was no state file to compare, so create recorded nothing') }
    if ($g.ExitCode -ne 0) { $p.Add('get failed') }
    if ((Get-ConformanceStateHash -ConformanceHome $h) -cne $stateBefore) { $p.Add('The state file changed') }
    Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-get: get changes nothing in the state file' -Problems $p

    # A failed write after the credential is placed: the state folder holds a folder where leases.json
    # goes, so the record cannot be written. Nothing may be left behind (lease-api.md#concurrency-and-atomicity).
    $badState = Join-Path $h.Root 'badstate'
    $badSsh = Join-Path $h.Root 'badssh'
    New-Item -ItemType Directory -Force -Path (Assert-ConformancePath -ConformanceHome $h -Path (Join-Path $badState 'leases.json')) | Out-Null
    $foldersBefore = Get-LeaseFolderNames -ConformanceHome $h
    $f = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'test-create') `
      -Environment @{ AGENTSPACE_STATE_DIR = $badState; AGENTSPACE_SSH_DIR = $badSsh }
    $p = Get-ConformanceErrorProblems -Run $f -Code internal
    Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-create: A create whose record cannot be written fails with internal, exit 1, nothing on stdout' -Problems $p
    $p = [System.Collections.Generic.List[string]]::new()
    if ($f.ExitCode -ne 1) { $p.Add('The create did not fail, so there is nothing to compare') }
    $leaked = @(Get-LeaseFolderNames -ConformanceHome $h | Where-Object { $foldersBefore -cnotcontains $_ })
    if ($leaked.Count -gt 0) { $p.Add("A lease folder was left: $($leaked -join ',')") }
    $aliasLeft = @(Get-ChildItem -LiteralPath (Join-Path $badSsh 'agentspace.d') -Force -ErrorAction SilentlyContinue)
    if ($aliasLeft.Count -gt 0) { $p.Add("An alias file was left: $($aliasLeft.Name -join ',')") }
    if (-not (Test-Path -LiteralPath (Join-Path $badState 'leases.json') -PathType Container)) { $p.Add('The state folder''s leases.json was replaced') }
    Complete-ConformanceCheck -Name 'lease-api.md#concurrency-and-atomicity: A failed create leaves no key, certificate, alias or record behind' -Problems $p
    $p = [System.Collections.Generic.List[string]]::new()
    if ($f.ExitCode -ne 1 -or -not $f.ErrorJson) { $p.Add('The create gave no error document to read') }
    foreach ($secret in $h.Root, $h.UserProfile, $env:USERNAME) {
      if ($secret -and $f.Stderr.Contains($secret)) { $p.Add('The error message holds an absolute path or the user name') ; break }
    }
    # No absolute path of any kind: a drive path or a UNC path (lease-api.md#exit-codes-and-errors).
    $said = if ($f.ErrorJson -and $f.ErrorJson.ContainsKey('message')) { [string]$f.ErrorJson['message'] } else { $f.Stderr }
    if ($said -match '[A-Za-z]:[\\/]' -or $said.Contains('\\')) { $p.Add('The error message holds an absolute path') }
    Complete-ConformanceCheck -Name 'lease-api.md#exit-codes-and-errors: An internal message holds no absolute path and no user name' -Problems $p

    # TTL, and two creates giving different IDs and aliases. It runs after the failed create, which MUST NOT block it.
    $second = Test-ReadyCreate -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'test-create', '--ttl', '90m') -Label 'A create with --ttl 90m' -Ttl 5400 -ExpectProject 'test-create'
    $made++
    $p = [System.Collections.Generic.List[string]]::new()
    if (-not $second.Doc -or $second.Doc['id'] -ceq $a['id']) { $p.Add('The two creates share an ID') }
    elseif ($second.Doc['ssh']['host_alias'] -ceq $alias) { $p.Add('The two creates share a host alias') }
    Complete-ConformanceCheck -Name 'lease-api.md#fields: Two creates get different IDs and different host aliases, and a create after a failed one works' -Problems $p

    # Expiry (lease-api.md#states): seed expires_at in the past, then in the far future, and read it with get.
    $seeded = Set-ConformanceRecordMember -ConformanceHome $h -Id $id -Member 'expires_at' -Value '2020-01-01T00:00:00Z'
    $hashSeeded = Get-ConformanceStateHash -ConformanceHome $h
    $e = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', $id)
    $p = Get-ConformanceSuccessProblems -Run $e
    if (-not $seeded) { $p.Add('The lease''s record could not be seeded in the state file') }
    if ($e.Json) {
      if ($e.Json['state'] -cne 'expired') { $p.Add("State is '$(ConvertTo-ConformanceMasked $e.Json['state'])', not 'expired'") }
      if ($e.Json.ContainsKey('ssh')) { $p.Add('An expired lease has ssh') }
      if ($e.Json['expires_at'] -cne '2020-01-01T00:00:00Z') { $p.Add('expires_at changed') }
      if ($e.Json['ttl_seconds'] -ne 28800) { $p.Add('ttl_seconds changed') }
    }
    Complete-ConformanceCheck -Name 'lease-api.md#states: A ready lease whose expires_at has passed is expired, without ssh, and keeps expires_at and ttl_seconds' -Problems $p
    $p = [System.Collections.Generic.List[string]]::new()
    if (-not $seeded -or -not $e.Json -or $e.Json['state'] -cne 'expired') { $p.Add('get did not read the seeded lease as expired, so there is nothing to compare') }
    if ((Get-ConformanceStateHash -ConformanceHome $h) -cne $hashSeeded) { $p.Add('get wrote the state file') }
    Complete-ConformanceCheck -Name 'state.md#the-lease-state-file: get never stores expired' -Problems $p
    $seeded = Set-ConformanceRecordMember -ConformanceHome $h -Id $id -Member 'expires_at' -Value '2099-01-01T00:00:00Z'
    $e = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', $id)
    $p = Get-ConformanceSuccessProblems -Run $e
    if (-not $seeded) { $p.Add('The lease''s record could not be seeded in the state file') }
    if (-not $e.Json -or $e.Json['state'] -cne 'ready' -or -not $e.Json['ssh']) { $p.Add("The lease is '$(ConvertTo-ConformanceMasked $(if ($e.Json) { $e.Json['state'] }))', not ready with ssh") }
    Complete-ConformanceCheck -Name 'lease-api.md#states: A ready lease whose expires_at is far ahead is ready, with ssh' -Problems $p
  } else {
    Write-ConformanceResult -Name 'lease-api.md#agentspace-lease-create: Every check of a ready lease (create, get, expiry, a failed write)' -Status SKIPPED -Reason $badSkip
  }

  # --- The time zone (lease-api.md#times) ------------------------------------------------------------
  # The suite MUST run at least once on a machine whose zone is not UTC. The harness cannot set a zone
  # for a child, so on a UTC machine a zone mix-up in the program stays hidden: report it, so that the
  # run is incomplete and never passed.
  if ([TimeZoneInfo]::Local.GetUtcOffset([DateTime]::UtcNow) -eq [TimeSpan]::Zero) {
    Write-ConformanceResult -Name 'lease-api.md#times: The run happens in a time zone that is not UTC' -Status SKIPPED -Reason 'The local UTC offset is 0, so a time zone mix-up in the program would stay hidden; run the suite where the zone is not UTC'
  }

  # --- An unsatisfiable lease: no CA is needed, and no credential is issued -------------------------
  # v0.12 (lease-api.md#platform, D98): linux/amd64 is a supported platform now, where xxl alone would be unsupported, so the
  # platform no backend supports here is windows/arm64, which lists unsupported_platform then unsupported_size (driver.md#what-it-supports).
  $u =Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'test-create', '--platform', 'windows/arm64', '--size', 'xxl')
  $made++
  $ua = $u.Json
  $p = Get-ConformanceSuccessProblems -Run $u
  if ($ua -and $ua['state'] -cne 'unsatisfiable') { $p.Add("State is '$($ua['state'])'") }
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-create: An unsatisfiable create exits 0 with state unsatisfiable, and validates' -Problems $p
  $p = [System.Collections.Generic.List[string]]::new()
  $codes = if ($ua -and $ua['reasons']) { @($ua['reasons'] | ForEach-Object { $_['code'] }) } else { @() }
  if (($codes -join ',') -cne 'unsupported_platform,unsupported_size') { $p.Add("The reasons are '$($codes -join ',')'") }
  elseif (-not ([string]$ua['reasons'][0]['message']).Contains('windows/arm64') -or -not ([string]$ua['reasons'][1]['message']).Contains('xxl')) { $p.Add('A reason''s message does not name the value that cannot be met') }
  if ($u.Stdout -match '(?i)\b(ec2|aws)\b') { $p.Add('The output names a backend') }
  Complete-ConformanceCheck -Name 'lease-api.md#fields: Its reasons are unsupported_platform then unsupported_size, in table order, each message naming its value' -Problems $p
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not $ua -or $u.ExitCode -ne 0) { $p.Add('The create gave no lease to look for') }
  if ($ua) {
    $uAlias = "agentspace-test-create-$($ua['id'].Substring(0, 8))"
    if (Get-LeaseFolderNames -ConformanceHome $h | Where-Object { $_ -ceq $ua['id'] }) { $p.Add('A lease folder was made') }
    if (Test-Path -LiteralPath (Get-AliasFile -ConformanceHome $h -Alias $uAlias)) { $p.Add('An alias file was made') }
  }
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-create: An unsatisfiable lease leaves no lease folder and no alias file' -Problems $p
  $ug = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', $(if ($ua) { [string]$ua['id'] } else { '' }))
  $p = Get-ConformanceSuccessProblems -Run $ug
  if (-not (Test-SameDocument $ug.Stdout $u.Stdout)) { $p.Add('get does not print the lease as create printed it, compared as parsed values') }
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-get: get returns the unsatisfiable lease as create printed it' -Problems $p

  # Each reason alone, and all three; a request is never met with a substitute (lease-api.md#agentspace-lease-create).
  $reasonCases = @(
    # windows/arm64 stays unsupported (lease-api.md#platform, D98), and lists unsupported_platform alone (lease-api.md#profile, pin P2).
    @{ Args = @('--platform', 'windows/arm64'); Codes = 'unsupported_platform' }
    @{ Args = @('--profile', 'agent-linux@1'); Codes = 'unsupported_profile' }
    @{ Args = @('--size', 'xxl'); Codes = 'unsupported_size' }
    @{ Args = @('--size', 'xxl', '--profile', 'agent-linux@1', '--platform', 'windows/arm64'); Codes = 'unsupported_platform,unsupported_profile,unsupported_size' }
  )
  foreach ($case in $reasonCases) {
    $line = @('lease', 'create', '--project', 'test-create') + $case.Args
    $c = Invoke-AgentspaceCli -ConformanceHome $h -Arguments $line
    $made++
    $p = Get-ConformanceSuccessProblems -Run $c
    $got = if ($c.Json -and $c.Json['reasons']) { @($c.Json['reasons'] | ForEach-Object { $_['code'] }) -join ',' } else { '' }
    if ($c.Json -and $c.Json['state'] -cne 'unsatisfiable') { $p.Add("State is '$(ConvertTo-ConformanceMasked $c.Json['state'])'") }
    if ($got -cne $case.Codes) { $p.Add("The reasons are '$(ConvertTo-ConformanceMasked $got)', not '$($case.Codes)'") }
    Complete-ConformanceCheck -Name "lease-api.md#fields: $(Format-ConformanceLine $line) is unsatisfiable with reasons $($case.Codes)" -Problems $p
  }

  # TTL as the lease records it, on an unsatisfiable lease so that no CA is needed (lease-api.md#ttl).
  foreach ($case in @(@('1m', 60), @('24h', 86400), @('1d', 86400), @('PT8H', 28800), @('1h 30m', 5400), @(' 90 MIN ', 5400))) {
    # windows/arm64 is unsupported (lease-api.md#platform, D98), so the lease is unsatisfiable and needs no CA.
    $line = @('lease', 'create', '--project', 'test-create', '--platform', 'windows/arm64', '--ttl', $case[0])
    $c = Invoke-AgentspaceCli -ConformanceHome $h -Arguments $line
    $made++
    $p = Get-ConformanceSuccessProblems -Run $c
    if ($c.Json -and $c.Json['ttl_seconds'] -ne $case[1]) { $p.Add("ttl_seconds is $(ConvertTo-ConformanceMasked $c.Json['ttl_seconds']), not $($case[1])") }
    Complete-ConformanceCheck -Name "lease-api.md#ttl: --ttl '$($case[0])' is $($case[1]) seconds" -Problems $p
  }
  $c = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'test-create', '--platform', 'windows/arm64')
  $made++
  $p = Get-ConformanceSuccessProblems -Run $c
  if ($c.Json -and $c.Json['ttl_seconds'] -ne 28800) { $p.Add("ttl_seconds is $(ConvertTo-ConformanceMasked $c.Json['ttl_seconds']), not 28800") }
  Complete-ConformanceCheck -Name 'lease-api.md#ttl: With no --ttl the TTL is 8 hours' -Problems $p

  # A tolerant command line (lease-api.md#input-tolerance): the values come back normalized.
  $line = @('Lease', 'Create', '--PROJECT=  X  ', '--platform', 'linux/aarch64/v8', '--profile', 'Agent-Linux@03', '--size', 'M', '--ttl', 'PT8H')
  $t = Invoke-AgentspaceCli -ConformanceHome $h -Arguments $line
  $made++
  $p = Get-ConformanceSuccessProblems -Run $t
  if ($t.Json) {
    $ta = $t.Json
    if ($ta['state'] -cne 'unsatisfiable' -or $ta['project'] -cne 'x' -or $ta['platform'] -cne 'linux/arm64' -or $ta['profile'] -cne 'agent-linux@3' -or $ta['size'] -cne 'm' -or $ta['ttl_seconds'] -ne 28800) {
      $p.Add('The values are not normalized to x, linux/arm64, agent-linux@3, m and 28800, unsatisfiable')
    }
    $codes = @($ta['reasons'] | ForEach-Object { $_['code'] }) -join ','
    if ($codes -cne 'unsupported_profile') { $p.Add("The reasons are '$codes', not unsupported_profile") }
  }
  Complete-ConformanceCheck -Name "lease-api.md#command-lines: $(Format-ConformanceLine $line) is read tolerantly and the values come back normalized" -Problems $p

  # not_found: a well-formed ID that names no lease (lease-api.md#lease-id), asked for in uppercase.
  $g = [guid]::NewGuid().ToString('N')
  $unknown = '{0}-{1}-4{2}-8{3}-{4}' -f $g.Substring(0, 8), $g.Substring(8, 4), $g.Substring(12, 3), $g.Substring(15, 3), $g.Substring(18, 12)
  $nf = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'get', $unknown.ToUpperInvariant())
  $p = Get-ConformanceErrorProblems -Run $nf -Code not_found -LeaseId $unknown.ToLowerInvariant()
  Complete-ConformanceCheck -Name 'lease-api.md#agentspace-lease-get: get of an unknown ID is not_found, exit 3, with the normalized lease_id' -Problems $p

  # The state file (state.md#the-lease-state-file), after every create above.
  $statePath = Get-ConformanceStatePath -ConformanceHome $h
  $p = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-Path -LiteralPath $statePath -PathType Leaf)) { $p.Add('There is no state file in AGENTSPACE_STATE_DIR') }
  else {
    $bytes = [IO.File]::ReadAllBytes($statePath)
    $text = [Text.UTF8Encoding]::new($false).GetString($bytes)
    if (-not (Test-ConformanceSchema -Json $text.TrimEnd("`n") -Schema 'state/leases.state.schema.json')) { $p.Add('The file does not validate against state/leases.state.schema.json') }
    if (Get-ConformanceHasBom $bytes) { $p.Add('The file starts with a byte order mark') }
    if (-not (Test-ConformanceOneLine $text) -or -not (Test-ConformanceJsonCompact $text.TrimEnd("`n"))) { $p.Add('The file is not compact JSON followed by one line feed') }
    $doc = Read-ConformanceStateFile -ConformanceHome $h
    if (@($doc.leases).Count -ne $made) { $p.Add("The file holds $(@($doc.leases).Count) records, not the $made that create recorded") }
    $noDriver = @($doc.leases | Where-Object { -not $_.driver })
    if ($noDriver.Count -gt 0) { $p.Add('A record has no driver') }
  }
  Complete-ConformanceCheck -Name 'state.md#the-lease-state-file: After the creates, <state folder>\leases.json validates against the state schema, is compact UTF-8 with one line feed and no BOM, holds one record per create, and each has a driver' -Problems $p

  # An unsatisfiable create needs no CA, and never makes one (state.md#creation-in-order). The throwaway CA is removed.
  $caDir = Join-Path $h.UserProfile '.agentspace\ca'
  if (Test-Path -LiteralPath $caDir) { Remove-Item -LiteralPath (Assert-ConformancePath -ConformanceHome $h -Path $caDir) -Recurse -Force }
  $n = Invoke-AgentspaceCli -ConformanceHome $h -Arguments @('lease', 'create', '--project', 'test-create', '--size', 'xxl')
  $p = Get-ConformanceSuccessProblems -Run $n
  if ($n.Json -and $n.Json['state'] -cne 'unsatisfiable') { $p.Add("State is '$(ConvertTo-ConformanceMasked $n.Json['state'])'") }
  if (Test-Path -LiteralPath $caDir) { $p.Add('create made a CA') }
  Complete-ConformanceCheck -Name 'state.md#creation-in-order: With no CA at all, an unsatisfiable create still answers, and makes no CA' -Problems $p
} catch {
  Write-ConformanceResult -Name 'test-create-get.ps1: The check ran to its end without an unexpected error' -Status FAIL -Reason "$(ConvertTo-ConformanceMasked $_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber)"
} finally {
  if ($h) { Remove-ConformanceHome -ConformanceHome $h }
}
Exit-ConformanceRun

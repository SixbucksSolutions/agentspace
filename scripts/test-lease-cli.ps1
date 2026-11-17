#Requires -Version 7
# RFC 2119 keywords in these comments; the capitals are load-bearing.
#
# Scripted check for the lease CLI's skeleton (story E1.02): scripts\lease-cli.ps1 and its shims,
# scripts\lease-values.ps1 and scripts\lease-state.ps1, against docs/lease-api.md.
# - It runs the real entry point as a separate process, through bin\agentspace.cmd and through
#   `pwsh -File scripts\lease-cli.ps1`, and captures stdout, stderr and the exit code as bytes.
# - Every --help level MUST be byte-identical to the help block that docs/lease-api.md gives for it,
#   read from the doc at test time, so the doc and the code cannot drift apart.
# - Every error document MUST validate against schemas/error.output.schema.json, with nothing on
#   stdout and the exit code of its code.
# - The normalizers MUST accept and refuse the spellings the contract lists.
# - The state file MUST round-trip, validate against schemas/state/leases.state.schema.json, give
#   lease documents that validate against schemas/lease.output.schema.json, survive a failed update
#   unchanged, and stay whole under concurrent writers and readers.
# It works only in a throwaway folder under $env:TEMP, named by AGENTSPACE_STATE_DIR, and MUST NOT
# read or write the real ~/.ssh, %USERPROFILE%\.agentspace or any real lease. It prints PASS or FAIL
# per check, removes its folder, and exits 1 on any failure. Laptop only.
# Run: pwsh -NoProfile -File scripts\test-lease-cli.ps1

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$cli = Join-Path $repo 'scripts\lease-cli.ps1'
$cmdShim = Join-Path $repo 'bin\agentspace.cmd'
$pwsh = (Get-Process -Id $PID).Path
$errorSchema = Join-Path $repo 'schemas\error.output.schema.json'
$leaseSchema = Join-Path $repo 'schemas\lease.output.schema.json'
$stateSchema = Join-Path $repo 'schemas\state\leases.state.schema.json'
$utf8 = [Text.UTF8Encoding]::new($false)
$failures = 0
$passes = 0

function Write-Result([bool]$Ok, [string]$Text) {
  if ($Ok) { Write-Output "PASS  $Text"; $script:passes++ } else { Write-Output "FAIL  $Text"; $script:failures++ }
}

# Returns 'valid', 'invalid' (rejected by the schema) or 'error: ...' (the schema or the JSON is broken).
function Test-Document([string]$Json, [string]$SchemaPath) {
  try {
    if (Test-Json -Json $Json -SchemaFile $SchemaPath -ErrorAction Stop) { return 'valid' }
    return 'invalid'
  } catch {
    $m = $_.Exception.Message
    if ($m -like '*is not valid with the schema*') { return 'invalid' }
    return "error: $m"
  }
}

function Format-Shown([string]$Text) { ($Text -replace "`r", '\r' -replace "`n", '\n') }

# Runs agentspace as its own process. Via 'cmd' runs bin\agentspace.cmd; via 'pwsh' runs
# pwsh -NoProfile -File scripts\lease-cli.ps1. Returns the exit code and stdout and stderr as bytes.
function Invoke-Agentspace([string]$Via, [string[]]$Arguments) {
  $psi = [Diagnostics.ProcessStartInfo]::new()
  if ($Via -eq 'cmd') {
    $psi.FileName = $cmdShim
  } else {
    $psi.FileName = $pwsh
    foreach ($a in '-NoProfile', '-File', $cli) { $psi.ArgumentList.Add($a) }
  }
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $p = [Diagnostics.Process]::Start($psi)
  $out = [IO.MemoryStream]::new()
  $err = [IO.MemoryStream]::new()
  $t1 = $p.StandardOutput.BaseStream.CopyToAsync($out)
  $t2 = $p.StandardError.BaseStream.CopyToAsync($err)
  $p.WaitForExit()
  [void]$t1.Wait(); [void]$t2.Wait()
  [pscustomobject]@{ Exit = $p.ExitCode; Out = $out.ToArray(); Err = $err.ToArray() }
}

function Show-Line([string[]]$Arguments) { 'agentspace ' + (($Arguments | ForEach-Object { if ($_ -match '\s' -or $_ -eq '') { "'$(Format-Shown $_)'" } else { $_ } }) -join ' ') }

# Checks a help run: exit 0, stdout byte-identical to the doc's block, nothing on stderr.
function Test-HelpRun([string]$Via, [string[]]$Arguments, [string]$Level) {
  $r = Invoke-Agentspace $Via $Arguments
  $want = $utf8.GetBytes($script:docHelp[$Level])
  $same = [Linq.Enumerable]::SequenceEqual([byte[]]$r.Out, [byte[]]$want)
  Write-Result ($r.Exit -eq 0 -and $same -and $r.Err.Length -eq 0) `
    "[$Via] $(Show-Line $Arguments) prints the '$Level' help from docs/lease-api.md (exit $($r.Exit), $($r.Out.Length) of $($want.Length) bytes, same: $same, stderr $($r.Err.Length) bytes)"
}

$exitFor = @{ internal = 1; usage = 2; invalid_value = 2; out_of_range = 2; not_found = 3; invalid_state = 4 }

# Checks an error run: the exit code of the code, nothing on stdout, one line on stderr that ends
# in one LF and validates against the error schema, and the code and field expected.
function Test-ErrorRun([string]$Via, [string[]]$Arguments, [string]$Code, [string]$Field = '', [string]$MessageLike = '') {
  $r = Invoke-Agentspace $Via $Arguments
  $text = $utf8.GetString($r.Err)
  $oneLine = $text.EndsWith("`n") -and $text.IndexOf("`n") -eq $text.Length - 1 -and -not $text.Contains("`r")
  $json = $text.TrimEnd("`n")
  $valid = Test-Document $json $script:errorSchema
  $doc = try { $json | ConvertFrom-Json -AsHashtable } catch { @{} }
  $ok = $r.Exit -eq $exitFor[$Code] -and $r.Out.Length -eq 0 -and $oneLine -and $valid -eq 'valid' -and
    $doc['code'] -ceq $Code -and ([string]$doc['field']) -ceq $Field -and
    $json -cnotmatch '\s(?=(?:[^"]*"[^"]*")*[^"]*$)' -and $r.Err[0] -ne 0xEF -and
    (-not $MessageLike -or $doc['message'] -clike $MessageLike)
  Write-Result $ok "[$Via] $(Show-Line $Arguments) fails with $Code$(if ($Field) { " ($Field)" }) (exit $($r.Exit), schema $valid, stdout $($r.Out.Length) bytes): $(Format-Shown $text)"
}

$stamp = Get-Date -Format 'yyyyMMddHHmmss'
$root = Join-Path $env:TEMP "agentspace-test-lease-cli-$stamp-$PID"
$savedStateDir = $env:AGENTSPACE_STATE_DIR
$savedSshDir = $env:AGENTSPACE_SSH_DIR
$savedProxy = $env:AGENTSPACE_PROXY_COMMAND
try {
  New-Item -ItemType Directory -Force -Path $root | Out-Null
  $env:AGENTSPACE_STATE_DIR = Join-Path $root 'state'
  # No check here is meant to place a lease, but one that does stays out of the real ~/.ssh.
  $env:AGENTSPACE_SSH_DIR = Join-Path $root 'ssh'
  $env:AGENTSPACE_PROXY_COMMAND = 'C:\Windows\System32\cmd.exe /d /c exit 1'

  # --- The help blocks, from the doc ---------------------------------------------------------
  $docLines = ([IO.File]::ReadAllText((Join-Path $repo 'docs\lease-api.md')) -replace "`r`n", "`n") -split "`n"
  $docHelp = @{}
  $section = $null
  for ($i = 0; $i -lt $docLines.Count; $i++) {
    if ($docLines[$i] -cmatch '^### (agentspace(?: lease(?: [a-z]+)?)?)$') { $section = $Matches[1]; continue }
    if ($docLines[$i] -ceq '**Help:**' -and $section) {
      $j = $i + 1
      while ($docLines[$j] -cne '```') { $j++ }
      $block = [Text.StringBuilder]::new()
      for ($k = $j + 1; $docLines[$k] -cne '```'; $k++) { [void]$block.Append($docLines[$k]).Append("`n") }
      $docHelp[$section] = $block.ToString()
      $section = $null
    }
  }
  $levels = [ordered]@{
    'agentspace'               = @()
    'agentspace lease'         = @('lease')
    'agentspace lease create'  = @('lease', 'create')
    'agentspace lease get'     = @('lease', 'get')
    'agentspace lease renew'   = @('lease', 'renew')
    'agentspace lease release' = @('lease', 'release')
    'agentspace lease list'    = @('lease', 'list')
  }
  Write-Result ($docHelp.Count -eq 7 -and @($levels.Keys | Where-Object { -not $docHelp[$_] }).Count -eq 0) `
    "docs/lease-api.md has a help block for each of the 7 levels ($($docHelp.Count) found)"

  # --- --help at every level, through both entry points ----------------------------------------
  foreach ($level in $levels.Keys) {
    foreach ($via in 'cmd', 'pwsh') {
      Test-HelpRun $via (@($levels[$level]) + '--help') $level
    }
    Test-HelpRun 'pwsh' (@($levels[$level]) + '-h') $level
  }

  # --- --help in odd positions ---------------------------------------------------------------
  $id = '3F2A9C1E-7B4D-4E6A-8C2F-0A1B2C3D4E5F'
  Test-HelpRun 'pwsh' @('lease', 'create', '--ttl', '--help') 'agentspace lease create'
  Test-HelpRun 'cmd' @('LEASE', 'CREATE', '-H') 'agentspace lease create'
  Test-HelpRun 'pwsh' @('lease', 'create', '--help', '--ttl', '99d') 'agentspace lease create'
  Test-HelpRun 'pwsh' @('lease', 'create', '--bogus', '--project', '-h') 'agentspace lease create'
  Test-HelpRun 'pwsh' @('lease', 'renew', $id, '--ttl', '-H') 'agentspace lease renew'
  Test-HelpRun 'pwsh' @('lease', 'list', '--state', 'ready', '--state', '--HELP') 'agentspace lease list'
  Test-HelpRun 'pwsh' @('Lease', '--bogus', '--Help') 'agentspace lease'
  Test-HelpRun 'pwsh' @('--help', 'frob') 'agentspace'
  Test-HelpRun 'pwsh' @('lease', '--help', 'frob') 'agentspace lease'

  # --- Usage errors --------------------------------------------------------------------------
  Test-ErrorRun 'cmd' @() usage
  Test-ErrorRun 'pwsh' @() usage
  Test-ErrorRun 'cmd' @('lease') usage
  Test-ErrorRun 'pwsh' @('frob') usage
  Test-ErrorRun 'pwsh' @('frob', '--help') usage
  Test-ErrorRun 'pwsh' @('lease', 'frob', '--help') usage
  Test-ErrorRun 'pwsh' @('--bogus') usage
  Test-ErrorRun 'pwsh' @('lease', 'create') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project=') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', '') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl', '--size', 'm') usage 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--Project=y') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', 'extra') usage
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--bogus') usage
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl:') usage
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '-ttl', '8h') usage
  Test-ErrorRun 'pwsh' @('lease', 'get') usage 'id'
  Test-ErrorRun 'pwsh' @('lease', 'get', $id, $id) usage
  Test-ErrorRun 'pwsh' @('lease', 'get', $id, '--ttl', '8h') usage
  Test-ErrorRun 'pwsh' @('lease', 'renew', $id, '--ttl', '8h', '--ttl', '9h') usage 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'release') usage 'id'
  Test-ErrorRun 'pwsh' @('lease', 'list', '--project', 'a', '--project', 'b') usage 'project'
  Test-ErrorRun 'pwsh' @('lease', 'list', 'extra') usage
  # A usage error is reported before a bad value, wherever each appears (contract error order).
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl', '8', '--bogus') usage

  # --- Values: invalid_value and out_of_range, in the order of each command's table ------------
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl', '8') invalid_value 'ttl' "TTL '8' has no unit.*"
  Test-ErrorRun 'cmd' @('lease', 'create', '--project', 'x', '--ttl', '48h') out_of_range 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl=99999999999999999999h') out_of_range 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--ttl=1:30') invalid_value 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'a_b') invalid_value 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', "a`nb") invalid_value 'project' "*'a\u000ab'*"
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'bad_', '--ttl', '8') invalid_value 'project'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--ttl', '8', '--project', 'x', '--size', 'toolarge9') invalid_value 'size'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--platform', 'linux') invalid_value 'platform'
  Test-ErrorRun 'pwsh' @('lease', 'create', '--project', 'x', '--profile', 'agent-linux@0') invalid_value 'profile'
  Test-ErrorRun 'pwsh' @('lease', 'get', 'not-a-uuid') invalid_value 'id'
  Test-ErrorRun 'pwsh' @('lease', 'release', '{3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f}') invalid_value 'id'
  Test-ErrorRun 'pwsh' @('lease', 'renew', 'nope', '--ttl', '8') invalid_value 'id'
  Test-ErrorRun 'pwsh' @('lease', 'renew', $id, '--ttl', '8') invalid_value 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'renew', $id, '--ttl', 'P2D') out_of_range 'ttl'
  Test-ErrorRun 'pwsh' @('lease', 'list', '--state', 'ready,bogus') invalid_value 'state'
  Test-ErrorRun 'pwsh' @('lease', 'list', '--project', '-x-') invalid_value 'project'

  # --- No command is a stub any more -------------------------------------------------------------
  # create and get are real since E1.04, and renew, release and list since E1.05. A command that
  # succeeds here would break the state-file checks below, so scripts\test-lease-cmd-create-get.ps1
  # and scripts\test-lease-cmd-renew-release-list.ps1 check them instead.

  # The error check is proven able to fail: a control character in a message breaks the schema.
  Write-Result ((Test-Document '{"code":"usage","message":"a\nb"}' $errorSchema) -eq 'invalid') 'The error schema refuses a message with a line feed in it'

  # --- The normalizers -----------------------------------------------------------------------
  . (Join-Path $repo 'scripts\lease-state.ps1')

  function Test-Accept([string]$Function, $Value, $Want) {
    try {
      $got = & $Function $Value
      $same = if ($Want -is [array]) { (@($got) -join ',') -ceq ($Want -join ',') } else { $got -ceq $Want -and $got.GetType() -eq $Want.GetType() }
      Write-Result $same "$Function '$(Format-Shown ($Value -join '|'))' gives '$(@($got) -join ',')'"
    } catch {
      Write-Result $false "$Function '$(Format-Shown ($Value -join '|'))' is accepted (threw: $($_.Exception.Message))"
    }
  }
  function Test-Refuse([string]$Function, $Value, [string]$Code, [string]$Field) {
    try {
      $got = & $Function $Value
      Write-Result $false "$Function '$(Format-Shown ($Value -join '|'))' is refused with $Code (gave '$got')"
    } catch {
      $e = Get-LeaseErrorException $_.Exception
      $doc = if ($e) { [ordered]@{ code = $e.Data['agentspace.lease.code']; message = $e.Message; field = $e.Data['agentspace.lease.field'] } | ConvertTo-Json -Compress } else { '' }
      $valid = if ($doc) { Test-Document $doc $errorSchema } else { 'none' }
      Write-Result ($e -and $e.Data['agentspace.lease.code'] -ceq $Code -and $e.Data['agentspace.lease.field'] -ceq $Field -and $valid -eq 'valid') `
        "$Function '$(Format-Shown ($Value -join '|'))' is refused with $Code ($Field), error document $valid"
    }
  }

  $v4 = '3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f'
  Test-Accept ConvertTo-LeaseIdValue $v4.ToUpperInvariant() $v4
  Test-Accept ConvertTo-LeaseIdValue " `t$v4 " $v4
  Test-Accept ConvertTo-LeaseIdValue '3f2a9c1e-7b4d-4e6a-bc2f-0a1b2c3d4e5f' '3f2a9c1e-7b4d-4e6a-bc2f-0a1b2c3d4e5f'
  foreach ($bad in '3f2a9c1e-7b4d-1e6a-8c2f-0a1b2c3d4e5f', '3f2a9c1e-7b4d-4e6a-cc2f-0a1b2c3d4e5f', "{$v4}", "urn:uuid:$v4", "$v4/x",
    '..\x', '', '3f2a9c1e7b4d4e6a8c2f0a1b2c3d4e5f') { Test-Refuse ConvertTo-LeaseIdValue $bad invalid_value id }

  Test-Accept ConvertTo-LeaseProjectValue 'SwimCoach' 'swimcoach'
  Test-Accept ConvertTo-LeaseProjectValue ' a ' 'a'
  Test-Accept ConvertTo-LeaseProjectValue 'A-9' 'a-9'
  Test-Accept ConvertTo-LeaseProjectValue ('a' * 32) ('a' * 32)
  Test-Accept ConvertTo-LeaseProjectValue "`tA`n" 'a'
  foreach ($bad in '-a', 'a-', 'a_b', ('a' * 33), '', ' ', 'a/b', 'a b', "a`nb", ([string][char]0x212A)) {
    Test-Refuse ConvertTo-LeaseProjectValue $bad invalid_value project
  }

  foreach ($pair in @(
      @('linux/arm64', 'linux/arm64'), @(' Linux/AArch64 ', 'linux/arm64'), @('linux/aarch64/v8', 'linux/arm64'),
      @('LINUX/ARM64/V8', 'linux/arm64'), @('linux/x86_64', 'linux/amd64'), @('linux/X86-64', 'linux/amd64'),
      @('linux/arm64/v9', 'linux/arm64/v9'), @('windows/amd64', 'windows/amd64'), @('linux/arm/v7', 'linux/arm/v7'),
      @('linux/amd64/v8', 'linux/amd64/v8'))) { Test-Accept ConvertTo-LeasePlatformValue $pair[0] $pair[1] }
  foreach ($bad in 'linux', 'linux/', '/arm64', 'linux/arm64/v8/x', 'linux/arm 64', '', 'linux\arm64', 'linux/x86_32') {
    Test-Refuse ConvertTo-LeasePlatformValue $bad invalid_value platform
  }

  foreach ($pair in @(@('Agent-Linux@03', 'agent-linux@3'), @('agent-linux', 'agent-linux'), @(' AGENT-LINUX ', 'agent-linux'),
      @('agent-linux@1', 'agent-linux@1'), @('x@10', 'x@10'), @(('p' * 64), ('p' * 64)))) { Test-Accept ConvertTo-LeaseProfileValue $pair[0] $pair[1] }
  foreach ($bad in 'agent-linux@0', 'agent-linux@00', 'agent-linux@', '@1', '-x', 'a@b', 'agent linux', ('p' * 65), '', 'a@1@2', 'a@-1') {
    Test-Refuse ConvertTo-LeaseProfileValue $bad invalid_value profile
  }

  foreach ($pair in @(@('M', 'm'), @(' xl ', 'xl'), @('12345678', '12345678'), @('m', 'm'))) { Test-Accept ConvertTo-LeaseSizeValue $pair[0] $pair[1] }
  foreach ($bad in '123456789', 'm-l', '', ' ', 'm.l') { Test-Refuse ConvertTo-LeaseSizeValue $bad invalid_value size }

  # TTL, from docs/lease-api.md "TTL": every spelling it lists, and every refusal.
  foreach ($pair in @(
      @('8h', 28800), @('90m', 5400), @('1h30m', 5400), @('1h 30m', 5400), @('2 hours', 7200), @('45 min', 2700),
      @('1d', 86400), @(' 8H ', 28800), @('1Min', 60), @('30 MINUTES', 1800), @('1 hr', 3600), @('2hrs', 7200),
      @('1 hour', 3600), @('1day', 86400), @('1 DAYS', 86400), @('3600s', 3600), @('100 secs', 100), @('90 SECONDS', 90),
      @('60 second', 60), @('2 mins', 120), @('1 minute', 60), @('1440m', 86400), @('23h59m60s', 86400), @('1d0h0m0s', 86400),
      @('1h1s', 3601), @('PT8H', 28800), @('pt90m', 5400), @('P1D', 86400), @('PT1H30M', 5400), @('p0dt1m', 60),
      @('PT86400S', 86400), @('PT60S', 60), @(' PT8H ', 28800))) {
    Test-Accept ConvertTo-LeaseTtlValue $pair[0] ([int]$pair[1])
  }
  foreach ($bad in '8', '28800', '', '   ', '8M', '1M', '1.5h', '-1h', '+1h', '1w', '1 week', '1y', '1mo', 'P1M', 'P1W', 'P1Y',
    'P', 'PT', 'P1DT', 'PT1D', '1h1h', '30m1h', '1s1m', '5ms', 'h', '8 hours ago', '1h,30m', 'PT1.5H', '8 h r', 'P 1D',
    'PT8H30', '1 1h', "1h`u{0660}") {
    Test-Refuse ConvertTo-LeaseTtlValue $bad invalid_value ttl
  }
  foreach ($bad in '0s', '59s', '0m', 'PT0S', 'PT59S', '24h1s', 'P1DT2H', '2d', '1441m', '99999999999999999999h',
    'PT99999999999999999999999999S') {
    Test-Refuse ConvertTo-LeaseTtlValue $bad out_of_range ttl
  }

  Test-Accept ConvertTo-LeaseStateFilterValue @('ready') @('ready')
  Test-Accept ConvertTo-LeaseStateFilterValue @(' Ready , PENDING ') @('ready', 'pending')
  Test-Accept ConvertTo-LeaseStateFilterValue @('ready', 'ready,expired', 'UNSATISFIABLE', 'released') @('ready', 'expired', 'unsatisfiable', 'released')
  foreach ($bad in @('bogus'), @('ready,'), @(''), @('ready,,pending'), @('ready', 'expire'), @('ready pending')) {
    Test-Refuse ConvertTo-LeaseStateFilterValue $bad invalid_value state
  }

  Test-Accept ConvertTo-LeaseTime ([DateTime]::new(2026, 11, 17, 18, 4, 5, 999, [DateTimeKind]::Utc)) '2026-11-17T18:04:05Z'
  Write-Result ((ConvertFrom-LeaseTime '2026-11-17T18:04:05Z') -eq [DateTime]::new(2026, 11, 17, 18, 4, 5, [DateTimeKind]::Utc)) 'ConvertFrom-LeaseTime reads a contract time as UTC'
  $now = Get-LeaseNow
  Write-Result ($now.Kind -eq 'Utc' -and $now.Millisecond -eq 0 -and ($now.Ticks % [TimeSpan]::TicksPerSecond) -eq 0) 'Get-LeaseNow is UTC and truncated to the whole second'

  # --- The state file ------------------------------------------------------------------------
  $statePath = Get-LeaseStatePath
  Write-Result ($statePath.StartsWith($root) -and $statePath.EndsWith('leases.json')) "The state file is in the throwaway folder: $statePath"
  Write-Result ((Read-LeaseState).Count -eq 0) 'A missing state file reads as no leases'

  $future = ConvertTo-LeaseTime ((Get-LeaseNow).AddHours(8))
  $past = '2026-01-02T03:04:05Z'
  $records = @(
    [ordered]@{ id = '11111111-1111-4111-8111-111111111111'; project = 'swimcoach'; state = 'pending'; platform = 'linux/arm64'
      profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800; created_at = '2026-11-17T18:04:05Z'; driver = 'opaque driver 1' }
    [ordered]@{ id = '22222222-2222-4222-9222-222222222222'; project = 'swimcoach'; state = 'ready'; platform = 'linux/arm64'
      profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800; created_at = '2026-11-17T18:04:05Z'; expires_at = $future
      ssh = [ordered]@{ host_alias = 'agentspace-swimcoach-22222222'; user = 'ubuntu' }; driver = 'opaque driver 1' }
    [ordered]@{ id = '33333333-3333-4333-a333-333333333333'; project = 'agentspace'; state = 'ready'; platform = 'linux/arm64'
      profile = 'agent-linux'; size = 'm'; ttl_seconds = 3600; created_at = '2026-01-02T02:04:05Z'; expires_at = $past
      ssh = [ordered]@{ host_alias = 'agentspace-agentspace-33333333'; user = 'ubuntu' }; driver = 'opaque driver 1' }
    [ordered]@{ id = '44444444-4444-4444-b444-444444444444'; project = 'swimcoach'; state = 'released'; platform = 'linux/arm64'
      profile = 'agent-linux'; size = 'm'; ttl_seconds = 60; created_at = '2026-11-17T18:04:05Z'; expires_at = '2026-11-17T18:05:05Z'
      released_at = '2026-11-17T18:04:35Z'; driver = 'opaque driver 1' }
    [ordered]@{ id = '55555555-5555-4555-8555-555555555555'; project = 'swimcoach'; state = 'released'; platform = 'linux/arm64'
      profile = 'agent-linux'; size = 'm'; ttl_seconds = 28800; created_at = '2026-11-17T18:04:05Z'; released_at = '2026-11-17T18:06:00Z' }
    [ordered]@{ id = '66666666-6666-4666-9666-666666666666'; project = 'swimcoach'; state = 'unsatisfiable'; platform = 'linux/amd64'
      profile = 'agent-linux@1'; size = 'xl'; ttl_seconds = 28800; created_at = '2026-11-17T18:04:05Z'; driver = 'opaque driver 1'
      reasons = @(
        [ordered]@{ code = 'unsupported_platform'; message = "Platform 'linux/amd64' cannot be met." }
        [ordered]@{ code = 'unsupported_profile'; message = "Profile 'agent-linux@1' cannot be met." }
        [ordered]@{ code = 'unsupported_size'; message = "Size 'xl' cannot be met." }) }
  )
  $returned = Update-LeaseState -Script { param($r) foreach ($x in $records) { $r.Add($x) }; 'done' }
  Write-Result ($returned -eq 'done') 'Update-LeaseState returns what its script block returns'
  $written = ([ordered]@{ version = 0; leases = $records } | ConvertTo-Json -Compress -Depth 20) + "`n"
  $fileBytes = [IO.File]::ReadAllBytes($statePath)
  $fileText = $utf8.GetString($fileBytes)
  Write-Result ($fileText -ceq $written) 'The state file holds exactly the records written, compact, with one LF at the end'
  Write-Result ($fileBytes[0] -ne 0xEF) 'The state file has no byte order mark'
  $r = Test-Document ([IO.File]::ReadAllText($statePath)) $stateSchema
  Write-Result ($r -eq 'valid') "The state file validates with Test-Json -SchemaFile leases.state.schema.json ($r)"

  $read = Read-LeaseState
  Write-Result ($read.Count -eq 6) "Read-LeaseState reads back every record ($($read.Count))"
  $readText = ([ordered]@{ version = 0; leases = $read.ToArray() } | ConvertTo-Json -Compress -Depth 20) + "`n"
  Write-Result ($readText -ceq $written) 'The records read back are identical to the records written'
  Write-Result ($read[1]['expires_at'] -is [string] -and $read[1]['expires_at'] -ceq $future -and $read[0]['created_at'] -ceq '2026-11-17T18:04:05Z') `
    'Times read back as the exact strings written, not as DateTime'

  $wantState = @('pending', 'ready', 'expired', 'released', 'released', 'unsatisfiable')
  for ($i = 0; $i -lt $read.Count; $i++) {
    $out = ConvertTo-LeaseOutput -Record $read[$i]
    $json = ConvertTo-Json -InputObject $out -Compress -Depth 20
    $v = Test-Document $json $leaseSchema
    Write-Result ($v -eq 'valid' -and $out['state'] -ceq $wantState[$i] -and -not $json.Contains('driver')) `
      "The $($read[$i]['state']) record $($read[$i]['id'].Substring(0, 8)) comes out $($out['state']), validates against lease.output.schema.json ($v), without driver: $json"
  }
  $expired = ConvertTo-LeaseOutput -Record $read[2]
  Write-Result (-not $expired.Contains('ssh') -and $expired['expires_at'] -ceq $past) 'A ready record past its expiry comes out expired, with expires_at and without ssh'
  $atExpiry = ConvertTo-LeaseOutput -Record $read[1] -Now (ConvertFrom-LeaseTime $future)
  $beforeExpiry = ConvertTo-LeaseOutput -Record $read[1] -Now ((ConvertFrom-LeaseTime $future).AddSeconds(-1))
  Write-Result ($atExpiry['state'] -ceq 'expired' -and $beforeExpiry['state'] -ceq 'ready') 'A ready lease is expired at its expires_at second, and ready one second before'
  $fromPending = ConvertTo-LeaseOutput -Record $read[4]
  Write-Result (-not $fromPending.Contains('expires_at')) 'A lease released from pending comes out without expires_at'

  # A failed update leaves the file as it was, byte for byte, and leaves no temporary file.
  $before = [IO.File]::ReadAllBytes($statePath)
  $threw = try { Update-LeaseState -Script { param($r) $r.RemoveAt(0); throw 'boom' }; $false } catch { $true }
  Write-Result ($threw -and [Linq.Enumerable]::SequenceEqual([byte[]]$before, [byte[]][IO.File]::ReadAllBytes($statePath))) `
    'An update whose script block throws changes nothing'
  $err = try { Update-LeaseState -Script { param($r) $r[1]['state'] = 'expired' }; $null } catch { Get-LeaseErrorException $_.Exception }
  Write-Result ($err -and $err.Data['agentspace.lease.code'] -ceq 'internal' -and
    [Linq.Enumerable]::SequenceEqual([byte[]]$before, [byte[]][IO.File]::ReadAllBytes($statePath))) `
    'An update that would store expired fails schema validation as internal, and changes nothing'
  $err = try { Update-LeaseState -Script { param($r) $r.Add($r[0]) }; $null } catch { Get-LeaseErrorException $_.Exception }
  Write-Result ($err -and [Linq.Enumerable]::SequenceEqual([byte[]]$before, [byte[]][IO.File]::ReadAllBytes($statePath))) `
    'An update that would store two records with one ID fails, and changes nothing'
  $err = try { Update-LeaseState -Script { param($r) throw (New-LeaseError -Code not_found -LeaseId $v4 -Message 'No lease has that ID.') }; $null } catch { Get-LeaseErrorException $_.Exception }
  Write-Result ($err -and $err.Data['agentspace.lease.code'] -ceq 'not_found' -and $err.Data['agentspace.lease.lease_id'] -ceq $v4) `
    'A lease error thrown inside an update reaches the caller unchanged'
  $leftovers = @(Get-ChildItem -LiteralPath (Split-Path -Parent $statePath) -Filter '*.tmp')
  Write-Result ($leftovers.Count -eq 0) "No temporary file is left behind ($($leftovers.Count))"
  Write-Result ((Find-LeaseRecord -Records $read -LeaseId $read[3]['id'])['state'] -ceq 'released' -and
    $null -eq (Find-LeaseRecord -Records $read -LeaseId $v4)) 'Find-LeaseRecord finds a record by ID, and $null for none'

  # A state file that is not valid reads as internal, never as no leases.
  $badDir = Join-Path $root 'bad-state'
  New-Item -ItemType Directory -Force -Path $badDir | Out-Null
  [IO.File]::WriteAllText((Join-Path $badDir 'leases.json'), '{"version":0,"leases":[{"id":"x"}]}', $utf8)
  $env:AGENTSPACE_STATE_DIR = $badDir
  $err = try { Read-LeaseState | Out-Null; $null } catch { Get-LeaseErrorException $_.Exception }
  Write-Result ($err -and $err.Data['agentspace.lease.code'] -ceq 'internal') 'A state file that does not validate is refused as internal'

  # Concurrent writers each add records under the lock while a reader reads the whole time:
  # every record MUST arrive, and the reader MUST never fail or see part of a file.
  $raceDir = Join-Path $root 'race-state'
  $env:AGENTSPACE_STATE_DIR = $raceDir
  $stateScript = Join-Path $repo 'scripts\lease-state.ps1'
  $writer = @"
`$ErrorActionPreference = 'Stop'
. '$stateScript'
1..5 | ForEach-Object {
  Update-LeaseState -Script { param(`$r) `$r.Add([ordered]@{ id = [guid]::NewGuid().ToString(); project = 'race'; state = 'pending'
    platform = 'linux/arm64'; profile = 'agent-linux'; size = 'm'; ttl_seconds = 60; created_at = '2026-11-17T18:04:05Z' }) } | Out-Null
}
"@
  $reader = @"
`$ErrorActionPreference = 'Stop'
. '$stateScript'
`$bad = 0; `$reads = 0; `$deadline = [DateTime]::UtcNow.AddSeconds(8)
while ([DateTime]::UtcNow -lt `$deadline) {
  try { [void](Read-LeaseState); `$reads++ } catch { `$bad++ }
  if (Test-Path -LiteralPath '$raceDir\done') { break }
}
"`$reads `$bad"
"@
  $encode = { param($s) [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($s)) }
  New-Item -ItemType Directory -Force -Path $raceDir | Out-Null
  $readerPsi = [Diagnostics.ProcessStartInfo]::new($pwsh)
  foreach ($a in '-NoProfile', '-EncodedCommand', (& $encode $reader)) { $readerPsi.ArgumentList.Add($a) }
  $readerPsi.UseShellExecute = $false
  $readerPsi.RedirectStandardOutput = $true
  $readerProc = [Diagnostics.Process]::Start($readerPsi)
  $writers = 1..3 | ForEach-Object {
    $psi = [Diagnostics.ProcessStartInfo]::new($pwsh)
    foreach ($a in '-NoProfile', '-EncodedCommand', (& $encode $writer)) { $psi.ArgumentList.Add($a) }
    $psi.UseShellExecute = $false
    [Diagnostics.Process]::Start($psi)
  }
  foreach ($w in $writers) { $w.WaitForExit() }
  New-Item -ItemType File -Path (Join-Path $raceDir 'done') | Out-Null
  $readerOut = $readerProc.StandardOutput.ReadToEnd().Trim()
  $readerProc.WaitForExit()
  $raceCount = (Read-LeaseState).Count
  $writerExits = ($writers | ForEach-Object { $_.ExitCode }) -join ','
  Write-Result ($raceCount -eq 15 -and $writerExits -eq '0,0,0') "Three concurrent writers of 5 records each leave 15 records ($raceCount; exits $writerExits)"
  $parts = $readerOut -split ' '
  Write-Result ($parts.Count -eq 2 -and [int]$parts[0] -gt 0 -and [int]$parts[1] -eq 0) "A concurrent reader never failed (reads, failures: $readerOut)"
} finally {
  $env:AGENTSPACE_STATE_DIR = $savedStateDir
  $env:AGENTSPACE_SSH_DIR = $savedSshDir
  $env:AGENTSPACE_PROXY_COMMAND = $savedProxy
  if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}

Write-Output "$passes passed, $failures failed."
if ($failures) { exit 1 }
exit 0

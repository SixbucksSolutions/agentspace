# Shared by every step script of the v0.12-T05 linux/amd64 live check (dot-sourced; drafter-owned, in target\t05).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# - Every step runs as `pwsh -NoProfile -File scripts\live\wrap.ps1 -Out target\runs\runner -Name <nnn>-<what> -Script <file>`.
#   Everything a step prints goes through ConvertTo-LiveMasked (scripts\live\mask.ps1); the wrapper masks the file again.
# - A lease's ID and alias live in raw files under the state folder (target\t05\s01-state\, or S01_STATE_DIR in a
#   dry run) and MUST NOT be printed: a step prints counts, True or False, and members by name. The raw document of a
#   create or get is kept there as doc-<step>.json.
# - Live mode (S01_DRY not 1): the programs are %USERPROFILE%\.agentspace\bin\agentspace.exe and agentspace-operator.exe,
#   and ssh and aws from the PATH. They are never listed, read, copied or hashed, and their paths are never printed.
#   A live run holding any S01_* or AGENTSPACE_* variable ends at once with exit 4 and runs nothing.
# - Dry mode (S01_DRY=1, the drafter's dry runs in a conformance-harness home): S01_AGENTSPACE, S01_OPERATOR, S01_SSH,
#   S01_AWS and S01_MASKPROOF name stand-in .ps1 files (run by pwsh -File), S01_AGENTSPACE_EXE names a build under test,
#   and S01_STATE_DIR, S01_OBSERVE_DIR, S01_GUARD_LOG, S01_PROVISION_LOG, S01_POLL_SECONDS and S01_BOUND_SECONDS move
#   the files and shorten the waits.
# - Exit codes: 0 PASS; 1 FAIL; 2 step 211 only, the lease ended not ready; 3 steps 211 and 240, the bound ran out;
#   4 STOP, a variable of the dry run in a live run; 6 the observer's hours for linux/amd64 reached 1.75, or could not
#   be read. Every nonzero exit prints a FINDING line first. A failure is a finding, never retried past.
# - Times shown are h:mmam in the laptop's zone, read with the clock and printed beside the zone's name.

$script:S01Dir = $PSScriptRoot
$script:S01Repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
. (Join-Path $script:S01Repo 'scripts\live\mask.ps1')

# What the check is about (docs\decisions.md D100, D94, D91). The TTL is pin P3's, pending: one place to change it.
$script:S01Project = 'agentspace'
$script:S01Platform = 'linux/amd64'
$script:S01HostName = 'agentspace.linux-amd64'
$script:S01Stack = 'agentspace-linux-amd64-host-agentspace'
$script:S01Ttl = '2h'
$script:S01TtlSeconds = 7200
$script:S01HostType = 'm8a.xlarge'
$script:S01HoursStop = 1.75
$script:S01DefaultObserve = 'C:\Projects\agentspace\target\t05-lead\observe'
$script:S01Utf8 = [Text.UTF8Encoding]::new($false)
$script:S01Inv = [Globalization.CultureInfo]::InvariantCulture

function Write-S01([string]$Text) {
  foreach ($line in ($Text -split "`r?`n")) {
    [Console]::Out.WriteLine((ConvertTo-LiveMasked -Text $line))
  }
  [Console]::Out.Flush()
}

function New-S01List { return , [System.Collections.Generic.List[string]]::new() }

# Runs a step's body and returns its exit code, the body's last output (an int). A throw is a FINDING and exit 1.
function Invoke-S01Main {
  param([scriptblock]$Body)
  try {
    $r = @(& $Body) | Select-Object -Last 1
    if ($r -isnot [int]) { Write-S01 'FINDING: The step ended without an exit code of its own.'; return 1 }
    return [int]$r
  } catch {
    Write-S01 ('FINDING: The step failed with a script error: ' + (($_.Exception.Message -replace '\s+', ' ')) + ' (line ' + $_.InvocationInfo.ScriptLineNumber + ').')
    return 1
  }
}

# The laptop's local time as h:mmam, read now, with the zone's name beside it.
function Get-S01Clock {
  [TimeZoneInfo]::ClearCachedData()
  $now = Get-Date
  return $now.ToString('h:mm', $script:S01Inv) + $now.ToString('tt', $script:S01Inv).ToLowerInvariant()
}
function Get-S01ZoneName { [TimeZoneInfo]::ClearCachedData(); return [TimeZoneInfo]::Local.Id }
function ConvertTo-S01Local([DateTimeOffset]$When) {
  [TimeZoneInfo]::ClearCachedData()
  $l = [TimeZoneInfo]::ConvertTime($When, [TimeZoneInfo]::Local)
  return $l.ToString('h:mm', $script:S01Inv) + $l.ToString('tt', $script:S01Inv).ToLowerInvariant()
}

function Test-S01Dry { return ($env:S01_DRY -ceq '1') }

# The names (never values) of the variables a live run MUST NOT hold.
function Get-S01EnvNames {
  @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -like 'S01_*' -or $_ -like 'AGENTSPACE_*' } | Sort-Object)
}

# Prints the step's title, the clock and the run's mode. Returns 4 when a live run holds a dry-run variable, else $null.
function Start-S01Step {
  param([string]$Step, [string]$What)
  Write-S01 "Step ${Step}: $What"
  Write-S01 "Clock: $(Get-S01Clock), zone $(Get-S01ZoneName)"
  $names = @(Get-S01EnvNames)
  if (Test-S01Dry) {
    Write-S01 "DRY RUN: variables held (names only): $($names -join ', ')"
    return $null
  }
  # S01_STATE_DIR alone is allowed in a live run: it only moves the raw state files, under target\.
  $names = @($names | Where-Object { $_ -cne 'S01_STATE_DIR' })
  if ($names.Count -gt 0) {
    Write-S01 "FINDING: The live run holds $($names.Count) dry-run or AGENTSPACE_* variable(s): $($names -join ', '); the installed programs run with none set, so nothing was started."
    return 4
  }
  return $null
}

# Prints the verdict and returns the exit code: 0 with no problems, else 1. A caller with another code prints its own FINDING.
function Complete-S01 {
  param([System.Collections.Generic.List[string]]$Problems)
  if ($Problems.Count -eq 0) { Write-S01 'PASS: every check of this step held.'; return 0 }
  foreach ($p in $Problems) { Write-S01 "FINDING: $p" }
  return 1
}

# --- The state folder, under the worktree's target\ --------------------------------------------------------------

function Get-S01StateDir {
  $d = if ($env:S01_STATE_DIR) { $env:S01_STATE_DIR } else { Join-Path $script:S01Dir 's01-state' }
  $full = [IO.Path]::GetFullPath($d)
  $target = (Join-Path $script:S01Repo 'target') + '\'
  if (-not ($full + '\').StartsWith($target, [StringComparison]::OrdinalIgnoreCase)) { throw 'The state folder MUST be under the worktree''s target\.' }
  New-Item -ItemType Directory -Force -Path $full | Out-Null
  return $full
}
function Get-S01StatePath([string]$Name) {
  if ($Name -cnotmatch '^[A-Za-z0-9._-]+$') { throw 'A state file name holds letters, digits, dots, underscores and dashes only.' }
  return (Join-Path (Get-S01StateDir) $Name)
}
function Set-S01State([string]$Name, [string]$Value) { [IO.File]::WriteAllBytes((Get-S01StatePath $Name), $script:S01Utf8.GetBytes($Value)) }
function Get-S01State([string]$Name) {
  $p = Get-S01StatePath $Name
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $null }
  return $script:S01Utf8.GetString([IO.File]::ReadAllBytes($p)).Trim()
}
# The lease's ID from its raw file, or $null. Never printed.
function Get-S01LeaseId([string]$Which) {
  $v = Get-S01State "lease-$Which.id"
  if ($v -and $v -cmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$') { return $v }
  return $null
}

# --- The programs ------------------------------------------------------------------------------------------------

# Returns File and Prefix for a tool. A path is never printed.
function Get-S01ToolSpec([string]$Tool) {
  $standin = switch ($Tool) { 'agentspace' { $env:S01_AGENTSPACE } 'operator' { $env:S01_OPERATOR } 'ssh' { $env:S01_SSH } 'aws' { $env:S01_AWS } 'maskproof' { $env:S01_MASKPROOF } default { $null } }
  if ((Test-S01Dry) -and $standin) {
    return @{ File = [Environment]::ProcessPath; Prefix = @('-NoProfile', '-NonInteractive', '-File', ([IO.Path]::GetFullPath($standin))) }
  }
  if ((Test-S01Dry) -and $Tool -ceq 'agentspace' -and $env:S01_AGENTSPACE_EXE) {
    return @{ File = [IO.Path]::GetFullPath($env:S01_AGENTSPACE_EXE); Prefix = @() }
  }
  switch ($Tool) {
    'maskproof' { return @{ File = [Environment]::ProcessPath; Prefix = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $script:S01Repo 'scripts\live\mask-proof.ps1')) } }
    'agentspace' { return @{ File = (Join-Path $env:USERPROFILE '.agentspace\bin\agentspace.exe'); Prefix = @() } }
    'operator' { return @{ File = (Join-Path $env:USERPROFILE '.agentspace\bin\agentspace-operator.exe'); Prefix = @() } }
    'ssh' { }
    'aws' { }
    default { throw "No such tool '$Tool'." }
  }
  $found = Get-Command -Name $Tool -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  return @{ File = $(if ($found) { [string]$found.Source } else { $Tool }); Prefix = @() }
}

# Runs one program with its streams captured and a bound. Returns ExitCode, Stdout, Stderr, StdoutBytes, StderrBytes,
# TimedOut, StartFailed. Only the process this call started is killed, by its own object.
function Invoke-S01Tool {
  param([string]$Tool, [string[]]$Arguments = @(), [int]$TimeoutSec = 120)
  $spec = Get-S01ToolSpec $Tool
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = [string]$spec.File
  foreach ($a in $spec.Prefix) { $psi.ArgumentList.Add([string]$a) }
  foreach ($a in $Arguments) { $psi.ArgumentList.Add([string]$a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  try { $p = [Diagnostics.Process]::Start($psi) }
  catch { return [pscustomobject]@{ ExitCode = -1; Stdout = ''; Stderr = ''; StdoutBytes = [byte[]]@(); StderrBytes = [byte[]]@(); TimedOut = $false; StartFailed = $true } }
  $out = [IO.MemoryStream]::new(); $err = [IO.MemoryStream]::new()
  $t1 = $p.StandardOutput.BaseStream.CopyToAsync($out)
  $t2 = $p.StandardError.BaseStream.CopyToAsync($err)
  try { $p.StandardInput.Close() } catch { }
  $timedOut = $false
  if (-not $p.WaitForExit($TimeoutSec * 1000)) { $timedOut = $true; try { $p.Kill($true) } catch { }; [void]$p.WaitForExit(10000) }
  [void]$t1.Wait(10000); [void]$t2.Wait(10000)
  $ob = $out.ToArray(); $eb = $err.ToArray()
  return [pscustomobject]@{ ExitCode = $(if ($timedOut) { -1 } else { $p.ExitCode }); Stdout = $script:S01Utf8.GetString($ob); Stderr = $script:S01Utf8.GetString($eb); StdoutBytes = $ob; StderrBytes = $eb; TimedOut = $timedOut; StartFailed = $false }
}

# A short masked excerpt of a failed run's streams, for a FINDING (the mask also runs in Write-S01).
function Get-S01Excerpt($Run) {
  $t = (($Run.Stderr + ' ' + $Run.Stdout) -replace '\s+', ' ').Trim()
  if ($t.Length -gt 400) { $t = $t.Substring(0, 400) + '...' }
  return (ConvertTo-LiveMasked -Text $t)
}

# --- JSON and lease documents ------------------------------------------------------------------------------------

function ConvertFrom-S01Json([string]$Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
  try { return ($Text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 50 -ErrorAction Stop) } catch { return $null }
}
function Test-S01Schema([string]$Json, [string]$Schema) {
  $file = Join-Path $script:S01Repo ('schemas\' + $Schema)
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "The schema $Schema is not under schemas." }
  if ([string]::IsNullOrWhiteSpace($Json)) { return $false }
  try { return [bool](Test-Json -Json $Json -SchemaFile $file -ErrorAction Stop) } catch { return $false }
}
function Test-S01Compact([string]$Text) {
  $inString = $false; $escaped = $false
  foreach ($ch in $Text.ToCharArray()) {
    if ($inString) { if ($escaped) { $escaped = $false } elseif ($ch -eq '\') { $escaped = $true } elseif ($ch -eq '"') { $inString = $false } }
    else { if ($ch -eq '"') { $inString = $true } elseif ($ch -eq ' ' -or $ch -eq "`t" -or $ch -eq "`r" -or $ch -eq "`n") { return $false } }
  }
  return $true
}
function Test-S01Uuid([object]$v) { return ($v -is [string] -and $v -cmatch '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$') }
function Test-S01Int($Value, [long]$Want) { return (($Value -is [int] -or $Value -is [long]) -and ([long]$Value -eq $Want)) }
function Get-S01Sorted($Table) { [string[]]$k = @($Table.Keys); [Array]::Sort($k, [StringComparer]::Ordinal); return , $k }
function ConvertTo-S01Time([object]$v) {
  if ($v -isnot [string] -or $v -cnotmatch '^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$') { return $null }
  $r = [DateTimeOffset]::MinValue
  if ([DateTimeOffset]::TryParse($v, $script:S01Inv, [Globalization.DateTimeStyles]::AssumeUniversal, [ref]$r)) { return $r }
  return $null
}

# Checks a run that should have printed one lease document (create, get or release). $Want keys: State, Size, Members
# (names), User ($null for no ssh), Ttl (seconds), Expires ('future', 'none' or 'any'), Id (the lease's ID or $null),
# NotId (another lease's ID or $null), Alias ('match' to check the alias form). Prints counts, members and True or False,
# never the ID or the alias. Returns the document as a hashtable, or $null.
function Test-S01Lease {
  param($Run, [hashtable]$Want, [System.Collections.Generic.List[string]]$Problems, [string]$StepName)
  if ($Run.StartFailed) { $Problems.Add('the program could not be started'); Write-S01 'Program: could not be started.'; return $null }
  Write-S01 "Exit: $($Run.ExitCode); stdout bytes: $($Run.StdoutBytes.Length); stderr bytes: $($Run.StderrBytes.Length)"
  if ($Run.TimedOut) { $Problems.Add('the program timed out'); return $null }
  if ($Run.ExitCode -ne 0) {
    $Problems.Add("exit $($Run.ExitCode), expected 0")
    if ($Run.StderrBytes.Length -gt 0) { Write-S01 ('Error document (masked): ' + (Get-S01Excerpt $Run)) }
    return $null
  }
  if ($Run.StderrBytes.Length -ne 0) { $Problems.Add("stderr holds $($Run.StderrBytes.Length) bytes") }
  $text = $Run.Stdout
  if (-not ($text.EndsWith("`n") -and $text.IndexOf("`n") -eq $text.Length - 1 -and -not $text.Contains("`r"))) { $Problems.Add('stdout is not one line ending in one line feed') }
  elseif (-not (Test-S01Compact $text.TrimEnd("`n"))) { $Problems.Add('stdout is not compact JSON') }
  if ($Run.StdoutBytes.Length -ge 3 -and $Run.StdoutBytes[0] -eq 0xEF -and $Run.StdoutBytes[1] -eq 0xBB -and $Run.StdoutBytes[2] -eq 0xBF) { $Problems.Add('stdout starts with a byte order mark') }
  $docText = $text.TrimEnd("`n")
  $doc = ConvertFrom-S01Json $docText
  if ($null -eq $doc -or $doc -isnot [hashtable]) { $Problems.Add('the document is not a JSON object'); Write-S01 'Document: not a JSON object.'; return $null }
  $wantMembers = [string[]]@($Want.Members); [Array]::Sort($wantMembers, [StringComparer]::Ordinal)
  $got = Get-S01Sorted $doc
  Write-S01 ('Members: ' + ($got -join ','))
  if (($got -join ',') -cne ($wantMembers -join ',')) { $Problems.Add("members are '$($got -join ',')', expected '$($wantMembers -join ',')'") }
  Write-S01 "State: '$($doc['state'])'; platform: '$($doc['platform'])'; profile: '$($doc['profile'])'; size: '$($doc['size'])'; project: '$($doc['project'])'; ttl_seconds: '$($doc['ttl_seconds'])'"
  if ($doc.ContainsKey('reasons') -and $doc['reasons'] -is [System.Collections.IEnumerable]) {
    $n = 0
    foreach ($r in @($doc['reasons'])) { $n++; if ($r -is [hashtable]) { Write-S01 "Reason ${n}: code=$($r['code']); message: $($r['message'])" } }
  }
  if ($doc['state'] -isnot [string] -or $doc['state'] -cne $Want.State) { $Problems.Add("state is '$($doc['state'])', expected '$($Want.State)'") }
  foreach ($pair in @(@('project', $script:S01Project), @('platform', $script:S01Platform), @('profile', 'agent-linux'), @('size', $Want.Size))) {
    if ($doc[$pair[0]] -isnot [string] -or $doc[$pair[0]] -cne $pair[1]) { $Problems.Add("$($pair[0]) is '$($doc[$pair[0]])', expected '$($pair[1])'") }
  }
  if (-not (Test-S01Int $doc['ttl_seconds'] $Want.Ttl)) { $Problems.Add("ttl_seconds is '$($doc['ttl_seconds'])', expected $($Want.Ttl)") }
  $idOk = Test-S01Uuid $doc['id']
  Write-S01 "id is a lowercase UUIDv4: $idOk"
  if (-not $idOk) { $Problems.Add('id is not a lowercase UUIDv4') }
  $wantId = if ($Want.ContainsKey('Id')) { $Want['Id'] } else { $null }
  $wantNotId = if ($Want.ContainsKey('NotId')) { $Want['NotId'] } else { $null }
  $wantAlias = if ($Want.ContainsKey('Alias')) { $Want['Alias'] } else { $null }
  if ($wantId) {
    $same = ($doc['id'] -is [string] -and $doc['id'] -ceq $wantId)
    Write-S01 "id is the lease's own, from the state file: $same"
    if (-not $same) { $Problems.Add('the document names another lease than the one in the state file') }
  }
  if ($wantNotId) {
    $other = ($doc['id'] -is [string] -and $doc['id'] -cne $wantNotId)
    Write-S01 "id differs from the other lease's: $other"
    if (-not $other) { $Problems.Add('the document names the same lease as the other one') }
  }
  $created = ConvertTo-S01Time $doc['created_at']
  if ($null -eq $created) { $Problems.Add('created_at is not a time of the contract''s form') }
  # expires_at
  $exp = $null
  if ($doc.ContainsKey('expires_at')) {
    $exp = ConvertTo-S01Time $doc['expires_at']
    if ($null -eq $exp) { $Problems.Add('expires_at is not a time of the contract''s form') }
  }
  if ($Want.Expires -ceq 'future' -and $null -ne $exp) {
    $left = ($exp - [DateTimeOffset]::UtcNow).TotalSeconds
    Write-S01 ('expires_at is ' + [Math]::Round($left / 60, 1) + ' minutes ahead (the TTL is ' + [Math]::Round($Want.Ttl / 60, 1) + ' minutes)')
    if ($left -le 0) { $Problems.Add('expires_at is not in the future') }
    if ($left -gt $Want.Ttl + 60) { $Problems.Add("expires_at is more than the TTL ahead: $([int]$left) seconds") }
    if ($null -ne $created -and ($exp - $created).TotalSeconds -lt $Want.Ttl - 1) { $Problems.Add('expires_at is less than the TTL after created_at') }
  }
  # ssh
  if ($null -eq $Want.User) {
    if ($doc.ContainsKey('ssh')) { $Problems.Add('the lease has an ssh member, expected none') }
    Write-S01 'ssh: absent'
  } elseif (-not $doc.ContainsKey('ssh') -or $doc['ssh'] -isnot [hashtable]) {
    $Problems.Add('the lease has no ssh object'); Write-S01 'ssh: absent'
  } else {
    $ssh = $doc['ssh']
    Write-S01 ('ssh members: ' + ((Get-S01Sorted $ssh) -join ',') + "; ssh.user: '$($ssh['user'])'")
    if (((Get-S01Sorted $ssh) -join ',') -cne 'host_alias,user') { $Problems.Add("ssh members are '$((Get-S01Sorted $ssh) -join ',')', expected 'host_alias,user'") }
    if ($ssh['user'] -isnot [string] -or $ssh['user'] -cne $Want.User) { $Problems.Add("ssh.user is '$($ssh['user'])', expected '$($Want.User)'") }
    $present = ($ssh['host_alias'] -is [string] -and $ssh['host_alias'].Length -gt 0)
    Write-S01 "ssh.host_alias present: $present"
    if (-not $present) { $Problems.Add('ssh.host_alias is absent or empty') }
    elseif ($wantAlias -ceq 'match') {
      $form = ($doc['id'] -is [string] -and $doc['id'].Length -ge 8 -and $ssh['host_alias'] -ceq ('agentspace-' + $script:S01Project + '-' + $doc['id'].Substring(0, 8)))
      Write-S01 "ssh.host_alias is agentspace-<project>-<first 8 hex digits of id>: $form"
      if (-not $form) { $Problems.Add('ssh.host_alias is not of the form agentspace-<project>-<first 8 hex digits of id>') }
    }
  }
  if ($Want.ContainsKey('Released') -and $Want['Released']) {
    $r = ConvertTo-S01Time $doc['released_at']
    Write-S01 "released_at is a time: $($null -ne $r)"
    if ($null -eq $r) { $Problems.Add('released_at is absent or not a time') }
  }
  $schemaOk = Test-S01Schema $docText 'lease.output.schema.json'
  Write-S01 "Schema lease.output.schema.json: $schemaOk"
  if (-not $schemaOk) { $Problems.Add('the document does not validate against lease.output.schema.json') }
  return $doc
}

# `lease list`: returns Leases (array of hashtables), Text and the counts, or $null (the problem is added).
function Read-S01LeaseList {
  param([System.Collections.Generic.List[string]]$Problems)
  $run = Invoke-S01Tool -Tool 'agentspace' -Arguments @('lease', 'list')
  Write-S01 'Command: agentspace lease list'
  if ($run.StartFailed) { $Problems.Add('lease list: the program could not be started'); return $null }
  Write-S01 "lease list exit: $($run.ExitCode)"
  if ($run.TimedOut -or $run.ExitCode -ne 0) { $Problems.Add("lease list exited $($run.ExitCode)"); return $null }
  if ($run.StderrBytes.Length -ne 0) { $Problems.Add("lease list wrote $($run.StderrBytes.Length) bytes to stderr") }
  $text = $run.Stdout.TrimEnd("`n")
  $doc = ConvertFrom-S01Json $text
  if ($null -eq $doc -or $doc -isnot [hashtable] -or -not $doc.ContainsKey('leases') -or $doc['leases'] -isnot [System.Collections.IEnumerable]) { $Problems.Add('lease list did not print an object with a leases array'); return $null }
  $valid = Test-S01Schema $text 'lease-list.output.schema.json'
  Write-S01 "Schema lease-list.output.schema.json: $valid"
  if (-not $valid) { $Problems.Add('the list does not validate against lease-list.output.schema.json') }
  $leases = @($doc['leases'] | Where-Object { $_ -is [hashtable] })
  $by = @{}
  foreach ($l in $leases) { $s = [string]$l['state']; if (-not $by.ContainsKey($s)) { $by[$s] = 0 }; $by[$s]++ }
  $fmt = if ($by.Count -eq 0) { 'none' } else { ((Get-S01Sorted $by) | ForEach-Object { "$_=$($by[$_])" }) -join ', ' }
  Write-S01 "Total leases: $($leases.Count); by state: $fmt"
  $hot = 0; foreach ($s in 'ready', 'pending') { if ($by.ContainsKey($s)) { $hot += $by[$s] } }
  $ours = @($leases | Where-Object { $_['project'] -ceq $script:S01Project -and $_['platform'] -ceq $script:S01Platform })
  $oursHot = @($ours | Where-Object { $_['state'] -ceq 'ready' -or $_['state'] -ceq 'pending' })
  $oursReady = @($ours | Where-Object { $_['state'] -ceq 'ready' })
  Write-S01 "Leases ready or pending, whole list: $hot; of $($script:S01Project) on $($script:S01Platform): $($ours.Count) (ready or pending $($oursHot.Count), ready $($oursReady.Count))"
  return @{ Leases = $leases; Hot = $hot; Ours = $ours; OursHot = $oursHot; OursReady = $oursReady }
}

# --- The observer's hours (scripts\live\observe-once.ps1 writes hours.json; Q20's cap is 2, the stop 1.75) ---------

function Get-S01ObserveDir { if ($env:S01_OBSERVE_DIR) { return [IO.Path]::GetFullPath($env:S01_OBSERVE_DIR) } return $script:S01DefaultObserve }

# Returns Ok, Hours and Why. Sums the intervals of the hosts named *.linux-amd64, an open one up to now.
function Get-S01Hours {
  $file = Join-Path (Get-S01ObserveDir) 'hours.json'
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { return [pscustomobject]@{ Ok = $false; Hours = 0.0; Why = 'the observer''s hours.json does not exist' } }
  $age = ([DateTime]::UtcNow - (Get-Item -LiteralPath $file).LastWriteTimeUtc).TotalMinutes
  if ($age -gt 10) { return [pscustomobject]@{ Ok = $false; Hours = 0.0; Why = "the observer's hours.json has not been written for $([int]$age) minutes, so the observer may be dead" } }
  $text = $null
  for ($try = 0; $try -lt 3 -and $null -eq $text; $try++) {
    try {
      $fs = [IO.File]::Open($file, 'Open', 'Read', 'ReadWrite')
      try { $text = [IO.StreamReader]::new($fs, $script:S01Utf8).ReadToEnd() } finally { $fs.Dispose() }
    } catch { Start-Sleep -Milliseconds 300 }
  }
  $items = $null
  if ($text) { try { $items = @($text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 10 -ErrorAction Stop) } catch { $items = $null } }
  if ($null -eq $items) { return [pscustomobject]@{ Ok = $false; Hours = 0.0; Why = 'the observer''s hours.json cannot be read' } }
  $now = [DateTimeOffset]::UtcNow
  $total = 0.0
  foreach ($i in $items) {
    if ($i -isnot [hashtable] -or [string]$i['host'] -cnotmatch '\.linux-amd64$') { continue }
    $s = ConvertTo-S01Time ([string]$i['start_utc'])
    if ($null -eq $s) { return [pscustomobject]@{ Ok = $false; Hours = 0.0; Why = 'an interval of hours.json has no readable start' } }
    $e = if ($i['end_utc']) { ConvertTo-S01Time ([string]$i['end_utc']) } else { $now }
    if ($null -eq $e) { return [pscustomobject]@{ Ok = $false; Hours = 0.0; Why = 'an interval of hours.json has no readable end' } }
    $total += ($e - $s).TotalHours
  }
  return [pscustomobject]@{ Ok = $true; Hours = $total; Why = '' }
}

# Prints the hours and returns 6 when they reach the stop (or cannot be read), else $null.
function Test-S01Hours([string]$Where) {
  $h = Get-S01Hours
  if (-not $h.Ok) {
    Write-S01 "FINDING: $Where, the instance hours of $($script:S01Platform) cannot be read: $($h.Why). Q20's cap is 2; the runner stops and asks the architect."
    return 6
  }
  Write-S01 ("Hours of {0} by the observer: {1:N3} (the stop is {2:N2}; Q20's cap is 2)" -f $script:S01Platform, $h.Hours, $script:S01HoursStop)
  if ($h.Hours -ge $script:S01HoursStop) {
    Write-S01 ("FINDING: $Where, the observer's hours of $($script:S01Platform) are {0:N3}, at or past {1:N2}; the runner stops and asks the architect before any step that could pass Q20's 2." -f $h.Hours, $script:S01HoursStop)
    return 6
  }
  return $null
}

# --- Logs, read only -----------------------------------------------------------------------------------------------

function Read-S01LogLines([string]$Path) {
  $text = $null
  try {
    $fs = [IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite, Delete')
    try { $text = [IO.StreamReader]::new($fs, $script:S01Utf8).ReadToEnd() } finally { $fs.Dispose() }
  } catch { return $null }
  return , @($text -split "`r?`n" | Where-Object { $_ -ne '' })
}
function Get-S01GuardLogPath { if ($env:S01_GUARD_LOG) { return [IO.Path]::GetFullPath($env:S01_GUARD_LOG) } return (Join-Path $env:USERPROFILE '.agentspace\guard.log') }
function Get-S01ProvisionLogPath { if ($env:S01_PROVISION_LOG) { return [IO.Path]::GetFullPath($env:S01_PROVISION_LOG) } return (Join-Path $env:USERPROFILE ('.agentspace\hosts\' + $script:S01HostName + '\provision.log')) }
# The guard.log's line count (a missing file counts 0: the log is created at its first line, operator.md, guard, Its log).
function Get-S01GuardCount { $l = Read-S01LogLines (Get-S01GuardLogPath); if ($null -eq $l) { return 0 }; return $l.Count }

# --- status ----------------------------------------------------------------------------------------------------------

# Parses status output into hosts, in the order printed. A report with no `Host` line (status -Host) is one host.
function ConvertFrom-S01Status([string]$Text) {
  $hosts = [System.Collections.Generic.List[hashtable]]::new()
  $cur = $null
  $leaseFileNote = $false
  foreach ($line in ($Text -split "\r?\n")) {
    if ($line -cmatch '^Host\s+(\S+) \((\S+)\), (\S+)\s*$') {
      $cur = @{ Name = $Matches[1]; Stack = $Matches[2]; Record = $Matches[3]; Platform = ''; State = ''; Type = ''; Ssm = ''; Leases = -1; Guard = $false; Instance = $false; Disk = $false }
      $hosts.Add($cur); continue
    }
    if ($null -eq $cur -and $line -cmatch '^(Platform|Instance|State|Type|Disk|SSM)\s') {
      $cur = @{ Name = '(host)'; Stack = ''; Record = ''; Platform = ''; State = ''; Type = ''; Ssm = ''; Leases = -1; Guard = $false; Instance = $false; Disk = $false }
      $hosts.Add($cur)
    }
    if ($null -eq $cur) { continue }
    if ($line -cmatch '^Platform\s+(\S+)') { $cur.Platform = $Matches[1] }
    elseif ($line -cmatch '^Instance\s+(\S.*)$') { $cur.Instance = $true }
    elseif ($line -cmatch '^State\s+(\S+)') { $cur.State = $Matches[1] }
    elseif ($line -cmatch '^Type\s+(\S+)') { $cur.Type = $Matches[1] }
    elseif ($line -cmatch '^Disk\s+\S') { $cur.Disk = $true }
    elseif ($line -cmatch '^SSM\s+(\S.*)$') { $cur.Ssm = $Matches[1].Trim() }
    elseif ($line -cmatch '^Leases\s+(\d+) live\s*$') { $cur.Leases = [int]$Matches[1] }
    elseif ($line -cmatch '^Guard\s+\S') { $cur.Guard = $true }
    if ($line -cmatch 'The lease state file .* cannot be read') { $leaseFileNote = $true }
  }
  return @{ Hosts = $hosts; LeaseFileNote = $leaseFileNote }
}

function Invoke-S01Status {
  param([string[]]$Arguments, [string]$Shown)
  Write-S01 "Command: agentspace-operator $Shown"
  $run = Invoke-S01Tool -Tool 'operator' -Arguments $Arguments -TimeoutSec 180
  if ($run.StartFailed) { return $run }
  Write-S01 "Exit: $($run.ExitCode)"
  $all = (($run.Stderr + "`n" + $run.Stdout) -split "\r?\n" | Where-Object { $_ -ne '' })
  foreach ($line in $all) { Write-S01 ('  ' + $line) }
  $run | Add-Member -NotePropertyName Text -NotePropertyValue ($all -join "`n") -Force
  return $run
}

# status -Host agentspace -Platform linux/amd64: used by 214 and by 220's second read.
function Test-S01HostStatus {
  param([System.Collections.Generic.List[string]]$Problems)
  $run = Invoke-S01Status -Arguments @('status', '-Host', $script:S01Project, '-Platform', $script:S01Platform) -Shown "status -Host $($script:S01Project) -Platform $($script:S01Platform)"
  if ($run.StartFailed) { $Problems.Add('status: the operator could not be started'); return }
  if ($run.TimedOut) { $Problems.Add('status timed out'); return }
  if ($run.ExitCode -ne 0) { $Problems.Add("status exited $($run.ExitCode), expected 0"); return }
  $r = ConvertFrom-S01Status $run.Text
  if ($r.Hosts.Count -ne 1) { $Problems.Add("status read $($r.Hosts.Count) hosts, expected 1"); return }
  $h = $r.Hosts[0]
  Write-S01 "Read: Platform '$($h.Platform)'; State '$($h.State)'; Type '$($h.Type)'; Instance line: $($h.Instance); Disk line: $($h.Disk); SSM '$($h.Ssm)'"
  if ($h.Platform -cne $script:S01Platform) { $Problems.Add("Platform is '$($h.Platform)', expected '$($script:S01Platform)'") }
  if ($h.State -cne 'running') { $Problems.Add("State is '$($h.State)', expected 'running'") }
  if ($h.Type -cne $script:S01HostType) { $Problems.Add("Type is '$($h.Type)', expected '$($script:S01HostType)'") }
  if (-not $h.Instance) { $Problems.Add('no Instance line') }
  if (-not $h.Disk) { $Problems.Add('no Disk line') }
  if ($h.Ssm -cne 'online') { Write-S01 "NOTE: SSM reads '$($h.Ssm)', not online; the host runs and the check goes on." }
}

# status -All (202 before, 250 after).
function Test-S01StatusAll {
  param([ValidateSet('before', 'after')][string]$Phase, [System.Collections.Generic.List[string]]$Problems)
  $run = Invoke-S01Status -Arguments @('status', '-All') -Shown 'status -All'
  if ($run.StartFailed) { $Problems.Add('status -All: the operator could not be started'); return }
  if ($run.TimedOut) { $Problems.Add('status -All timed out'); return }
  if ($run.ExitCode -ne 0) { $Problems.Add("status -All exited $($run.ExitCode), expected 0") }
  $r = ConvertFrom-S01Status $run.Text
  $hosts = $r.Hosts
  Write-S01 "Hosts read: $($hosts.Count); State lines: $(@($hosts | Where-Object { $_.State }).Count); Leases lines: $(@($hosts | Where-Object { $_.Leases -ge 0 }).Count); Guard lines: $(@($hosts | Where-Object { $_.Guard }).Count)"
  if ($hosts.Count -eq 0) { $Problems.Add('no host was read'); return }
  foreach ($h in $hosts) {
    $line = "  host $($h.Name): record $($h.Record), State '$($h.State)', Type '$($h.Type)', Leases $(if ($h.Leases -ge 0) { $h.Leases } else { 'none' })"
    Write-S01 $line
    if ($h.State -cne 'stopped') { $Problems.Add("host $($h.Name) is '$($h.State)', not stopped") }
    if ($h.Leases -lt 0) { $Problems.Add("host $($h.Name) has no Leases line") }
    elseif ($h.Leases -ne 0) { $Problems.Add("host $($h.Name) has $($h.Leases) live lease(s), expected 0") }
    if ($h.Guard) { $Problems.Add("host $($h.Name) has a Guard line, so it runs unexplained") }
  }
  if ($r.LeaseFileNote) { $Problems.Add('status says the lease state file cannot be read') }
  # The arm64 hosts, by name, MUST be there and stopped (a missing one would be read as stopped by a loose check).
  foreach ($n in @(@('swimcoach', 'agentbox'), @('agentspace', 'agentspace-host-agentspace'))) {
    $h = @($hosts | Where-Object { $_.Name -ceq $n[0] -and $_.Stack -ceq $n[1] })
    Write-S01 "linux/arm64 host '$($n[0])' ($($n[1])) listed: $($h.Count -eq 1)"
    if ($h.Count -ne 1) { $Problems.Add("the linux/arm64 host '$($n[0])' is not listed once") }
  }
  $amd = @($hosts | Where-Object { $_.Name -ceq $script:S01HostName })
  Write-S01 "$($script:S01HostName) listed: $($amd.Count)"
  if ($Phase -ceq 'after') {
    if ($amd.Count -ne 1) { $Problems.Add("$($script:S01HostName) is not listed once, so the check did not make it") }
    elseif ($amd[0].Stack -cne $script:S01Stack -or $amd[0].Platform -cne $script:S01Platform) { $Problems.Add("$($script:S01HostName) is not stack $($script:S01Stack) on $($script:S01Platform)") }
  } elseif ($amd.Count -gt 1) { $Problems.Add("$($script:S01HostName) is listed $($amd.Count) times") }
}

# Shared by every step script of E0.12-T05-S02's windows/amd64 live check (dot-sourced; drafter-owned, in target\t05).
#
# RFC 2119 keywords; the capitals are load-bearing.
#
# - Everything a step prints goes through Write-S02Line, which first replaces a Windows computer name of the EC2 form
#   (EC2AMAZ-XXXXXXX, which scripts\live\mask.ps1 does not cover) with <win-host>, then passes the line through
#   ConvertTo-LiveMasked. A step MUST NOT print any other way, and MUST NOT print a lease ID or alias (the literals
#   <lease> and <alias> stand in), a raw program output, `ssh -G`'s user line, or a file's content.
# - A lease ID, alias and document live only in raw state files under the state folder (target\t05\s02-state by default),
#   which a reader MUST NOT print.
# - Exit codes of a step: 0 PASS; 1 FINDING (a check failed; each one printed as a FINDING: line); 2 the lease ended in
#   a state that is not ready (311); 3 the loop's bound was reached (311 and 341); 5 STOP, a precondition failed before
#   anything ran; 6 windows/amd64's instance hours reached 4.5 (every loop).
# - Real mode: the installed programs (%USERPROFILE%\.agentspace\bin\agentspace.exe and agentspace-operator.exe), the
#   PATH's ssh, scp and aws, and C:\Projects\localswim\localswim.exe. A real run refuses (STOP) while any AGENTSPACE_*
#   variable is set, since one would name another program or folder. A step never lists, reads, copies or hashes an
#   installed program: it only starts it.
# - Dry mode (a dry run's driver sets S02_DRY=1): S02_STANDIN_<TOOL> names a .ps1 that stands in for a tool (AGENTSPACE,
#   OPERATOR, SSH, SCP, AWS, LOCALSWIM), S02_PROGRAM_<TOOL> a build's .exe (never the installed one), S02_STATE the state
#   folder, S02_OBSERVE the observer's folder, S02_ENVLIB the board credential's loader, S02_MASKPROOF the mask proof.
#   The timing knobs S02_POLL_SECONDS, S02_MAX_SECONDS, S02_IDLE_SECONDS, S02_STOP_SECONDS, S02_HOLD_SECONDS and
#   S02_SLEEP_SECONDS shorten a loop. A live run sets none of these.

$script:S02Dir = $PSScriptRoot
$script:S02Repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:S02Utf8 = [Text.UTF8Encoding]::new($false)
$script:S02Problems = [System.Collections.Generic.List[string]]::new()
$script:S02WindowsHost = 'agentspace.windows-amd64'
$script:S02Project = 'agentspace'
$script:S02OperatorAlias = 'agentspace-host.agentspace.windows-amd64'
. (Join-Path $script:S02Repo 'scripts\live\mask.ps1')

function ConvertTo-S02Safe([string]$Text) {
  $t = [regex]::Replace($Text, '(?i)\bEC2AMAZ-[A-Z0-9]{3,}\b', '<win-host>')
  $t = [regex]::Replace($t, '(?i)\bWIN-[A-Z0-9]{8,}\b', '<win-host>')
  return $t
}

function Write-S02Line([string]$Text) {
  foreach ($l in ($Text -split "\r?\n")) {
    [Console]::Out.WriteLine((ConvertTo-LiveMasked -Text (ConvertTo-S02Safe $l)))
  }
  [Console]::Out.Flush()
}

function Get-S02LocalText([DateTime]$Utc) {
  [TimeZoneInfo]::ClearCachedData()
  $l = [TimeZoneInfo]::ConvertTimeFromUtc($Utc.ToUniversalTime(), [TimeZoneInfo]::Local)
  $inv = [Globalization.CultureInfo]::InvariantCulture
  return $l.ToString('h:mm', $inv) + $l.ToString('tt', $inv).ToLowerInvariant()
}

function Get-S02NowText { return (Get-S02LocalText ([DateTime]::UtcNow)) }

# 2026-11-27T21:41:07Z as a UTC DateTime, or $null.
function ConvertFrom-S02Utc([string]$Text) {
  $out = [DateTime]::MinValue
  $style = [Globalization.DateTimeStyles]::AdjustToUniversal -bor [Globalization.DateTimeStyles]::AssumeUniversal
  if ([DateTime]::TryParseExact($Text, "yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture, $style, [ref]$out)) { return $out }
  return $null
}

# A UTC time as `h:mmam` in the laptop's zone, with the source's own form beside it.
function Format-S02Time([string]$UtcText) {
  $d = ConvertFrom-S02Utc $UtcText
  if ($null -eq $d) { return $UtcText }
  return ((Get-S02LocalText $d) + ' (' + $UtcText + ')')
}

# A log line that starts with a UTC stamp, as `h:mmam (stamp) rest`; any other line is returned as it is.
function Format-S02LogLine([string]$Line) {
  $m = [regex]::Match($Line, '^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ) (.*)$')
  if (-not $m.Success) { return $Line }
  return ((Format-S02Time $m.Groups[1].Value) + ' ' + $m.Groups[2].Value)
}

function Add-S02Finding([string]$Text) {
  $script:S02Problems.Add($Text)
  Write-S02Line "FINDING: $Text"
}

function Assert-S02([bool]$Condition, [string]$Ok, [string]$Finding) {
  if ($Condition) { Write-S02Line "ok: $Ok" } else { Add-S02Finding $Finding }
}

function Complete-S02 {
  if ($script:S02Problems.Count -eq 0) { Write-S02Line 'PASS: every check of this step held.'; return 0 }
  Write-S02Line "FAIL: $($script:S02Problems.Count) finding(s)."
  return 1
}

function Test-S02Dry { return ($env:S02_DRY -eq '1') }

function Get-S02Knob([string]$Name, [int]$Default) {
  $v = [Environment]::GetEnvironmentVariable($Name)
  $n = 0
  if (-not [string]::IsNullOrEmpty($v) -and [int]::TryParse($v, [ref]$n)) { return $n }
  return $Default
}

# Prints the step's heading, the time and the zone as the system reads them, and refuses a real run that holds an
# AGENTSPACE_* variable. Returns $false for a STOP, and the caller MUST then exit 5.
function Start-S02Step([string]$Name, [string]$What) {
  Write-S02Line "Step ${Name}: $What"
  [TimeZoneInfo]::ClearCachedData()
  Write-S02Line ('Time: ' + (Get-S02NowText) + ' (laptop zone: ' + [TimeZoneInfo]::Local.Id + ')')
  if (-not (Test-S02Dry)) {
    $names = @([Environment]::GetEnvironmentVariables().Keys | ForEach-Object { [string]$_ } | Where-Object { $_ -like 'AGENTSPACE_*' } | Sort-Object)
    if ($names.Count -gt 0) {
      Write-S02Line "STOP: the live run holds $($names.Count) AGENTSPACE_* variable(s): $($names -join ', '); the installed programs run with none set."
      return $false
    }
  }
  return $true
}

function Stop-S02Step([string]$Reason) {
  Write-S02Line "STOP: $Reason"
}

# --- State files (raw; never printed) -------------------------------------------------------------------------------

function Get-S02StateDir {
  $dir = if ($env:S02_STATE) { [IO.Path]::GetFullPath($env:S02_STATE) } else { Join-Path $script:S02Repo 'target\t05\s02-state' }
  if (-not ($dir.TrimEnd('\') + '\').StartsWith((Join-Path $script:S02Repo 'target') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The state folder MUST be under the worktree''s target\.'
  }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  return $dir
}

function Get-S02StatePath([string]$Name) { return (Join-Path (Get-S02StateDir) $Name) }

function Test-S02State([string]$Name) { return (Test-Path -LiteralPath (Get-S02StatePath $Name) -PathType Leaf) }

function Read-S02State([string]$Name) {
  $p = Get-S02StatePath $Name
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return $null }
  return [IO.File]::ReadAllText($p, $script:S02Utf8).Trim()
}

function Write-S02State([string]$Name, [string]$Text) {
  [IO.File]::WriteAllBytes((Get-S02StatePath $Name), $script:S02Utf8.GetBytes($Text))
}

# --- The tools --------------------------------------------------------------------------------------------------------

function Get-S02InstalledBin { return (Join-Path $env:USERPROFILE '.agentspace\bin') }

# The file and leading arguments that start a tool. Never printed.
function Get-S02ToolSpec([string]$Tool) {
  $upper = $Tool.ToUpperInvariant()
  $standin = [Environment]::GetEnvironmentVariable("S02_STANDIN_$upper")
  if (-not [string]::IsNullOrEmpty($standin)) {
    return @{ File = [Environment]::ProcessPath; Prefix = @('-NoProfile', '-NonInteractive', '-File', ([IO.Path]::GetFullPath($standin))) }
  }
  $program = [Environment]::GetEnvironmentVariable("S02_PROGRAM_$upper")
  if (-not [string]::IsNullOrEmpty($program)) {
    $full = [IO.Path]::GetFullPath($program)
    if ($full.StartsWith((Get-S02InstalledBin), [StringComparison]::OrdinalIgnoreCase)) { throw "S02_PROGRAM_$upper MUST NOT name the installed program." }
    return @{ File = $full; Prefix = @() }
  }
  switch ($Tool) {
    'agentspace' { return @{ File = (Join-Path (Get-S02InstalledBin) 'agentspace.exe'); Prefix = @() } }
    'operator' { return @{ File = (Join-Path (Get-S02InstalledBin) 'agentspace-operator.exe'); Prefix = @() } }
    'localswim' { return @{ File = 'C:\Projects\localswim\localswim.exe'; Prefix = @() } }
    default {
      $cmd = Get-Command -Name $Tool -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
      if ($null -eq $cmd) { throw "The program $Tool is not on the PATH." }
      return @{ File = $cmd.Source; Prefix = @() }
    }
  }
}

# Starts a tool without waiting. stdin is written when given and closed. Returns a handle for Wait-S02Tool.
function Start-S02Tool {
  param([string]$Tool, [string[]]$Arguments = @(), [string]$StdinText)
  $spec = Get-S02ToolSpec $Tool
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $spec.File
  foreach ($a in $spec.Prefix) { $psi.ArgumentList.Add($a) }
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $started = [DateTime]::UtcNow
  try {
    $p = [Diagnostics.Process]::Start($psi)
  } catch {
    return [pscustomobject]@{ Process = $null; Out = $null; Err = $null; Tasks = @(); Started = $started; StartError = 'the program could not be started' }
  }
  $out = [IO.MemoryStream]::new()
  $err = [IO.MemoryStream]::new()
  $tasks = @($p.StandardOutput.BaseStream.CopyToAsync($out), $p.StandardError.BaseStream.CopyToAsync($err))
  try {
    if ($StdinText) { $b = $script:S02Utf8.GetBytes($StdinText); $p.StandardInput.BaseStream.Write($b, 0, $b.Length) }
    $p.StandardInput.Close()
  } catch { }
  return [pscustomobject]@{ Process = $p; Out = $out; Err = $err; Tasks = $tasks; Started = $started; StartError = $null }
}

# Waits for a started tool, killing only its own process tree at the limit. Returns ExitCode, Stdout, Stderr, TimedOut,
# ElapsedSeconds, EndedUtc.
function Wait-S02Tool {
  param($Handle, [int]$TimeoutSeconds = 300)
  if ($null -eq $Handle.Process) {
    return [pscustomobject]@{ ExitCode = -1; Stdout = ''; Stderr = $Handle.StartError; TimedOut = $false; ElapsedSeconds = 0.0; EndedUtc = [DateTime]::UtcNow }
  }
  $p = $Handle.Process
  $timedOut = $false
  if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
    $timedOut = $true
    try { $p.Kill($true) } catch { }
    [void]$p.WaitForExit(10000)
  }
  foreach ($t in $Handle.Tasks) { try { [void]$t.Wait(10000) } catch { } }
  $ended = [DateTime]::UtcNow
  return [pscustomobject]@{
    ExitCode = $(if ($timedOut) { -1 } else { $p.ExitCode })
    Stdout = $script:S02Utf8.GetString($Handle.Out.ToArray())
    Stderr = $script:S02Utf8.GetString($Handle.Err.ToArray())
    TimedOut = $timedOut
    ElapsedSeconds = ($ended - $Handle.Started).TotalSeconds
    EndedUtc = $ended
  }
}

function Invoke-S02Tool {
  param([string]$Tool, [string[]]$Arguments = @(), [int]$TimeoutSeconds = 300, [string]$StdinText)
  $h = Start-S02Tool -Tool $Tool -Arguments $Arguments -StdinText $StdinText
  return (Wait-S02Tool -Handle $h -TimeoutSeconds $TimeoutSeconds)
}

# ssh options of every probe: never a prompt, a bounded connect, and no forwarding of the laptop's board ports.
function Get-S02SshOptions([switch]$KeepForwardings) {
  $o = @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=30')
  if (-not $KeepForwardings) { $o += @('-o', 'ClearAllForwardings=yes') }
  return $o
}

function Start-S02Ssh {
  param([string]$Alias, [string]$Command, [switch]$KeepForwardings)
  $a = @(Get-S02SshOptions -KeepForwardings:$KeepForwardings) + @($Alias, $Command)
  return (Start-S02Tool -Tool 'ssh' -Arguments $a)
}

function Invoke-S02Ssh {
  param([string]$Alias, [string]$Command, [int]$TimeoutSeconds = 180, [switch]$KeepForwardings)
  $h = Start-S02Ssh -Alias $Alias -Command $Command -KeepForwardings:$KeepForwardings
  return (Wait-S02Tool -Handle $h -TimeoutSeconds $TimeoutSeconds)
}

# `S02 key=value` lines of a probe's output, as an ordered list of pairs, and a lookup of each key's values.
function ConvertFrom-S02Probe([string]$Text) {
  $pairs = [System.Collections.Generic.List[object]]::new()
  $by = @{}
  foreach ($line in ($Text -split "\n")) {
    $l = $line.TrimEnd("`r")
    $m = [regex]::Match($l, '^S02 ([A-Za-z0-9_.-]+)=(.*)$')
    if (-not $m.Success) { continue }
    $k = $m.Groups[1].Value
    $v = $m.Groups[2].Value
    $pairs.Add([pscustomobject]@{ Key = $k; Value = $v })
    if (-not $by.ContainsKey($k)) { $by[$k] = [System.Collections.Generic.List[string]]::new() }
    $by[$k].Add($v)
  }
  return [pscustomobject]@{ Pairs = $pairs; By = $by }
}

function Get-S02First($Probe, [string]$Key) {
  if ($Probe.By.ContainsKey($Key)) { return $Probe.By[$Key][0] }
  return $null
}

# Copies a host script to the host's home and runs it there under PowerShell 7 by its name; the script removes itself.
# Returns the ssh result and the parsed probe, or $null when the copy failed (the finding is added).
function Invoke-S02HostScript {
  param([string]$Alias, [string]$Name, [int]$TimeoutSeconds = 240, [switch]$Quiet, [switch]$Tolerant, [string]$ScriptArgs = '')
  $local = Join-Path $script:S02Dir "s02-host-$Name.ps1"
  $remote = ".s02-host-$Name.ps1"
  $copy = Invoke-S02Tool -Tool 'scp' -Arguments (@('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=30', '-o', 'ClearAllForwardings=yes', '-q', $local, ($Alias + ':' + $remote))) -TimeoutSeconds 120
  if ($copy.ExitCode -ne 0) {
    if (-not $Tolerant) { Add-S02Finding "The copy of host script $Name failed (scp exit $($copy.ExitCode))." }
    return $null
  }
  $argText = if ($ScriptArgs) { ' ' + $ScriptArgs } else { '' }
  $run = Invoke-S02Ssh -Alias $Alias -Command ("pwsh -NoProfile -NonInteractive -File $remote$argText # s02:host-$Name") -TimeoutSeconds $TimeoutSeconds
  if (-not $Quiet) {
    foreach ($line in ($run.Stdout -split "\n")) { $l = $line.TrimEnd("`r"); if ($l -ne '') { Write-S02Line ("host: " + $l) } }
  }
  if ($run.ExitCode -ne 0) {
    if ($Tolerant) { return $null }
    Add-S02Finding "Host script $Name exited $($run.ExitCode)."
  }
  return [pscustomobject]@{ Run = $run; Probe = (ConvertFrom-S02Probe $run.Stdout) }
}

# --- JSON, the lease documents and the host's files on the laptop ---------------------------------------------------

function ConvertFrom-S02Json([string]$Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) { return $null }
  try { return ($Text | ConvertFrom-Json -AsHashtable -DateKind String -Depth 50 -ErrorAction Stop) } catch { return $null }
}

function Test-S02Schema([string]$Json, [string]$Schema) {
  $file = Join-Path $script:S02Repo ('schemas\' + $Schema)
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "The schema $Schema is not under schemas." }
  try { return [bool](Test-Json -Json $Json -SchemaFile $file -ErrorAction Stop) } catch { return $false }
}

function Get-S02SortedKeys($Table) {
  [string[]]$k = @($Table.Keys)
  [Array]::Sort($k, [StringComparer]::Ordinal)
  return , $k
}

function Get-S02ProvisionLogPath { return (Join-Path $env:USERPROFILE ('.agentspace\hosts\' + $script:S02WindowsHost + '\provision.log')) }
function Get-S02GuardLogPath { return (Join-Path $env:USERPROFILE '.agentspace\guard.log') }

# The lines of a log another process appends to, read with sharing; a missing file is an empty list.
function Read-S02LogLines([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @() }
  $fs = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  try { $text = [IO.StreamReader]::new($fs, $script:S02Utf8).ReadToEnd() } finally { $fs.Dispose() }
  return @($text -split "`n" | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ -ne '' })
}

# --- The observer's hours -------------------------------------------------------------------------------------------

function Get-S02ObserveDir {
  if ($env:S02_OBSERVE) { return [IO.Path]::GetFullPath($env:S02_OBSERVE) }
  return 'C:\Projects\agentspace\target\t05-lead\observe'
}

# windows/amd64's instance hours from the observer's hours.json (observe-once.ps1's header), an open interval summed up
# to now. Returns $null when the file is missing or cannot be read.
function Get-S02WindowsHours {
  $file = Join-Path (Get-S02ObserveDir) 'hours.json'
  if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { return $null }
  $fs = [IO.File]::Open($file, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  try { $text = [IO.StreamReader]::new($fs, $script:S02Utf8).ReadToEnd() } finally { $fs.Dispose() }
  $doc = ConvertFrom-S02Json $text
  if ($null -eq $doc) { return $null }
  $total = 0.0
  $now = [DateTime]::UtcNow
  foreach ($i in @($doc)) {
    if ($i -isnot [System.Collections.IDictionary]) { continue }
    if ([string]$i['host'] -cne $script:S02WindowsHost) { continue }
    $s = ConvertFrom-S02Utc ([string]$i['start_utc'])
    if ($null -eq $s) { continue }
    $e = if ($i['end_utc']) { ConvertFrom-S02Utc ([string]$i['end_utc']) } else { $now }
    if ($null -eq $e) { $e = $now }
    $total += ($e - $s).TotalHours
  }
  return $total
}

# True (with a FINDING printed) when the hours reached 4.5; the loop's caller then exits 6.
function Test-S02HoursReached {
  $h = Get-S02WindowsHours
  if ($null -eq $h) { return $false }
  if ($h -ge 4.5) {
    Add-S02Finding ('windows/amd64''s instance hours reached ' + $h.ToString('0.00', [Globalization.CultureInfo]::InvariantCulture) + ', the loop''s limit of 4.5 (Q20 allows 5); the loop stops and the architect is asked first.')
    return $true
  }
  return $false
}

function Get-S02HoursText {
  $h = Get-S02WindowsHours
  if ($null -eq $h) { return 'n/a' }
  return $h.ToString('0.000', [Globalization.CultureInfo]::InvariantCulture)
}

# --- The operator's status and the lease list -------------------------------------------------------------------------

# Runs `agentspace-operator status -All` and parses its host blocks. Returns Exit, Hosts (each Name, State, Platform,
# Leases) and the masked text lines.
function Read-S02StatusAll {
  $r = Invoke-S02Tool -Tool 'operator' -Arguments @('status', '-All') -TimeoutSeconds 240
  $text = ($r.Stdout + "`n" + $r.Stderr)
  $hosts = [System.Collections.Generic.List[object]]::new()
  $cur = $null
  foreach ($line in ($text -split "\r?\n")) {
    $m = [regex]::Match($line, '^Host\s{6}(\S+) \(([^)]*)\), (\S+)')
    if ($m.Success) {
      $cur = [pscustomobject]@{ Name = $m.Groups[1].Value; Record = $m.Groups[3].Value; State = $null; Platform = $null; Leases = $null }
      $hosts.Add($cur)
      continue
    }
    if ($null -eq $cur) { continue }
    if ($line -match '^State\s{5}(\S+)') { $cur.State = $Matches[1] }
    elseif ($line -match '^Platform\s{2}(\S+)') { $cur.Platform = $Matches[1] }
    elseif ($line -match '^Leases\s{4}(\d+) live') { $cur.Leases = [int]$Matches[1] }
  }
  return [pscustomobject]@{ Exit = $r.ExitCode; Hosts = $hosts; Text = $text; TimedOut = $r.TimedOut }
}

# Runs `agentspace lease list` and counts. Returns Exit, Total, ByState, WindowsByState or $null on a parse failure.
function Read-S02LeaseList {
  $r = Invoke-S02Tool -Tool 'agentspace' -Arguments @('lease', 'list') -TimeoutSeconds 120
  if ($r.ExitCode -ne 0) { Add-S02Finding "lease list exited $($r.ExitCode)."; return $null }
  $doc = ConvertFrom-S02Json $r.Stdout.TrimEnd("`n")
  if ($null -eq $doc -or $doc -isnot [System.Collections.IDictionary] -or -not $doc.ContainsKey('leases')) { Add-S02Finding 'lease list did not print an object with a leases array.'; return $null }
  $by = @{}
  $win = @{}
  $n = 0
  foreach ($l in @($doc['leases'])) {
    if ($l -isnot [System.Collections.IDictionary]) { continue }
    $n++
    $s = [string]$l['state']
    if (-not $by.ContainsKey($s)) { $by[$s] = 0 }
    $by[$s]++
    if ([string]$l['platform'] -ceq 'windows/amd64') { if (-not $win.ContainsKey($s)) { $win[$s] = 0 }; $win[$s]++ }
  }
  return [pscustomobject]@{ Exit = $r.ExitCode; Total = $n; ByState = $by; WindowsByState = $win }
}

function Format-S02Counts($Table) {
  if ($Table.Count -eq 0) { return 'none' }
  return ((Get-S02SortedKeys $Table | ForEach-Object { "$_=$($Table[$_])" }) -join ', ')
}

# 302 and 350: every host's state printed (masked), the hosts that must be stopped checked, and the lease list counted.
function Invoke-S02StatusAndList {
  param([bool]$EveryHostStopped, [bool]$RequireWindowsHost = $false, [string[]]$PrintOnly = @('agentspace.linux-amd64'))
  $st = Read-S02StatusAll
  Write-S02Line "status -All exit: $($st.Exit)"
  foreach ($line in ($st.Text -split "\r?\n")) { if ($line -ne '') { Write-S02Line ('  ' + $line) } }
  Assert-S02 ($st.Exit -eq 0 -and -not $st.TimedOut) 'status -All exited 0.' "status -All exited $($st.Exit)$(if ($st.TimedOut) { ' (timed out)' })."
  Write-S02Line "Hosts read: $($st.Hosts.Count)"
  Assert-S02 ($st.Hosts.Count -gt 0) 'At least one host was read.' 'No Host line was read from status -All.'
  foreach ($h in $st.Hosts) {
    $state = if ($h.State) { $h.State } else { 'none' }
    if ($PrintOnly -contains $h.Name) { Write-S02Line "Host $($h.Name): state $state (printed, not a check unless every host must be stopped)" }
    if ($EveryHostStopped -or $PrintOnly -notcontains $h.Name) {
      Assert-S02 ($state -ceq 'stopped') "Host $($h.Name) is stopped." "Host $($h.Name) is $state, not stopped."
    }
    if ($h.Name -ceq $script:S02WindowsHost -and $h.Leases -ne $null) { Write-S02Line "Host $($h.Name): $($h.Leases) live lease(s)" }
  }
  $win = @($st.Hosts | Where-Object { $_.Name -ceq $script:S02WindowsHost })
  Write-S02Line "Windows host in the registry: $($win.Count -gt 0)"
  if ($RequireWindowsHost) { Assert-S02 ($win.Count -eq 1) 'The Windows host is in the registry once.' "The Windows host is in the registry $($win.Count) time(s), expected 1." }
  $ll = Read-S02LeaseList
  if ($null -ne $ll) {
    Write-S02Line "Total leases: $($ll.Total)"
    Write-S02Line ('Leases by state: ' + (Format-S02Counts $ll.ByState))
    Write-S02Line ('windows/amd64 leases by state: ' + (Format-S02Counts $ll.WindowsByState))
    $hot = 0
    foreach ($s in 'ready', 'pending') { if ($ll.ByState.ContainsKey($s)) { $hot += $ll.ByState[$s] } }
    Assert-S02 ($hot -eq 0) 'No lease is ready or pending.' "$hot lease(s) are ready or pending; a host may be running."
  }
}

# --- The step's frame ---------------------------------------------------------------------------------------------------

$script:S02Exit = $null

# A step that ends on a code other than 0 or 1 (2, 3 or 6) sets it here, after printing its FINDING.
function Set-S02Exit([int]$Code) { $script:S02Exit = $Code }

# Runs a step's body, turns an error into a FINDING, prints the verdict, and returns the step's exit code.
function Invoke-S02Main([scriptblock]$Body) {
  try { [void](& $Body) } catch { Add-S02Finding ('The step stopped on an error: ' + $_.Exception.Message) }
  $code = Complete-S02
  if ($null -ne $script:S02Exit -and $script:S02Exit -ne 0) { return $script:S02Exit }
  return $code
}
